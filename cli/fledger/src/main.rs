use std::{collections::HashMap, fs::File, io::Write, path::PathBuf, sync::Arc, time::{Duration, Instant}};

use clap::Parser;

use flarch::{
    broker::{Broker, BrokerError},
    data_storage::{DataStorage, DataStorageFile},
    nodeids::NodeID,
    tasks::{now, wait_ms},
    web_rtc::connection::{ConnectionConfig, HostLogin, Login},
};
use flmodules::{
    gossip_events::core::{Category, Event},
    loopix::{
        broker::LoopixBroker, config::{CoreConfig, LoopixConfig, LoopixRole}, storage::LoopixStorage, END_TO_END_LATENCY, LOOPIX_START_TIME, NUMBER_OF_PROXY_REQUESTS
    },
    network::{messages::NetworkMessage, network_broker_start, signal::SIGNAL_VERSION},
    nodeconfig::NodeInfo,
    overlay::{broker::loopix::OverlayLoopix, messages::OverlayMessage},
    web_proxy::{broker::WebProxy, core::WebProxyConfig},
};
use flnode::{node::Node, version::VERSION_STRING};
use prometheus::{gather, Encoder, TextEncoder};
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey, StaticSecret};

/// Fledger node CLI binary
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Path to the configuration directory
    #[clap(short, long, default_value = "./flnode")]
    config: String,

    /// Set the name of the node - reverts to a random value if not given
    #[clap(short, long)]
    name: Option<String>,

    /// Uptime interval - to stress test disconnections
    #[clap(short, long)]
    uptime_sec: Option<usize>,

    /// Signalling server URL
    #[clap(short, long, default_value = "wss://signal.fledg.re")]
    signal_url: String,

    /// Verbosity of the logger
    #[clap(flatten)]
    verbosity: clap_verbosity_flag::Verbosity,

    #[clap(short, long)]
    path_len: Option<usize>,

    /// Number of clients
    #[clap(short, long)]
    n_clients: Option<usize>,

    /// Reliability config
    #[clap(short = 'r', long="retry", default_value_t = 0)]
    retry: u8,

    /// Save metrics to a new file every 10 seconds if true, otherwise overwrite
    #[clap(short = 'm', long="save_new_metrics_file", default_value_t = false)]
    save_new_metrics_file: bool,

    /// Time to start loopix
    #[clap(short = 't', long="start_loopix_time", default_value_t = 0)]
    start_loopix_time: u64,

    /// Number of duplicates to send
    #[clap(short = 'd', long="duplicates", default_value_t = 1)]
    duplicates: u8,

    /// Token to use for the request
    #[clap(short = 'k', long="token", default_value = "")]
    token: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();
    
    let args = Args::parse();
    let config = args.config.clone();
    let retry = args.retry;
    let duplicates = args.duplicates;
    let n_clients = if let Some(n_clients) = args.n_clients {
        n_clients
    } else {
        panic!("Number of clients must be provided");
    };

    let mut logger = env_logger::Builder::new();
    logger.filter_module("fl", args.verbosity.log_level_filter());
    
    logger.parse_env("RUST_LOG");
    logger.try_init().expect("Failed to initialize logger");

    let storage = DataStorageFile::new(args.config, "fledger".into());
    let mut node_config = Node::get_config(storage.clone())?;
    args.name.map(|name| node_config.info.name = name);

    log::info!(
        "Starting app with version {}/{}",
        SIGNAL_VERSION,
        VERSION_STRING
    );
    log::info!("start loopix time is {}", args.start_loopix_time);
    log::info!("retry is {}", retry);
    log::info!("duplicates is {}", duplicates);

    log::debug!("Connecting to websocket at {}", args.signal_url);
    let network = network_broker_start(
        node_config.clone(),
        ConnectionConfig::new(
            Some(args.signal_url),
            None,
            Some(HostLogin {
                url: "turn:web.fledg.re:3478".into(),
                login: Some(Login {
                    user: "something".into(),
                    pass: "something".into(),
                }),
            }),
        ),
    )
    .await?;
    let mut node = Node::start(Box::new(storage), node_config, network, Some(config.clone())).await?;
    let nc = node.node_config.info.clone();


    let mut storage_path = PathBuf::from(".");
    storage_path.push(config.clone());
    storage_path.push("loopix_storage.yaml");

    let mut state;
    if storage_path.exists() {

        state = LoopixSimul::Child(LSChild::WaitConfig);
        log::info!(
            "Starting node with previous state {:?} {}: {}",
            state,
            nc.get_id(),
            nc.name
        );

    } else {
        state = match args.path_len {
            Some(len) => LoopixSimul::Root(LSRoot::WaitNodes(len, n_clients)),
            None => LoopixSimul::Child(LSChild::WaitConfig),
        };
        log::info!(
            "Starting new node with state {:?} {}: {}",
            state,
            nc.get_id(),
            nc.name
        );
    }

    log::info!("Started successfully");
    let mut i: u32 = 0;
    loop {
        i += 1;
        log::info!("We go to process!");
        node.process()
            .await
            .err()
            .map(|e| log::warn!("Couldn't process node: {e:?}"));

        state = state.process(args.token.clone(), &mut node, i, retry, duplicates, start_time, args.start_loopix_time, args.save_new_metrics_file).await?;

        wait_ms(1000).await;
    }
}

async fn save_metrics_loop(
    storage: Arc<LoopixStorage>,
    dir_path: String,
    save_new_metrics_file: bool,
    start_time: Instant,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    log::info!("Starting save metrics thread!");
    let path = PathBuf::from(dir_path);

    let mut storage_path = PathBuf::from(".");
    storage_path.push(path.clone());
    if let Err(e) = std::fs::create_dir_all(&storage_path) {
        log::error!("Failed to create directory: {}", e);
    }
    storage_path.push("loopix_storage.yaml");

    let mut metrics_base_path = PathBuf::from(".");
    metrics_base_path.push(path);
    if let Err(e) = std::fs::create_dir_all(&metrics_base_path) {
        log::error!("Failed to create directory: {}", e);
    }

    let storage_bytes = storage.to_yaml_async().await.unwrap();
    {
        let mut file = File::create(&storage_path).expect("Unable to create file");
        file.write_all(storage_bytes.as_bytes())
            .expect("Unable to write data");
    }
    let sleep_time = if save_new_metrics_file {
        60
    } else {
        5
    };

    let mut i: u32 = 0;
    loop {
        std::thread::sleep(Duration::from_secs(sleep_time));

        let mut metrics_path = metrics_base_path.clone();
        if save_new_metrics_file {
            log::info!("Saving metrics to new file {} at {} seconds since start", i, start_time.elapsed().as_secs_f64());
            metrics_path.push(format!("metrics_{}.txt", i));
        } else {
            log::info!("Saving metrics to existing file for the {}th time at {} seconds since start", i, start_time.elapsed().as_secs_f64());
            metrics_path.push("metrics.txt");
        }

        let metric_families = gather();
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();

        {
            let mut file = File::create(&metrics_path).expect("Unable to create file");
            file.write_all(&buffer).expect("Unable to write data");
        }

        i += 1;
    }
}

// State-machine for the loopix simulation
#[derive(Debug, PartialEq, Clone, Copy)]
enum LoopixSimul {
    // It's the root node, which sets up the configuration
    Root(LSRoot),
    // It's a child node
    Child(LSChild),
}

impl LoopixSimul {
    async fn process(&self, token: String, node: &mut Node, i: u32, retry: u8, duplicates: u8, start_time: Instant, loopix_start_time: u64, save_new_metrics_file: bool) -> Result<Self, BrokerError> {
        let new_state = match &self {
            LoopixSimul::Root(lsroot) => lsroot.process(token, node, i, retry,  duplicates, start_time, loopix_start_time, save_new_metrics_file).await?,
            LoopixSimul::Child(lschild) => lschild.process(token, node, i, retry, duplicates, start_time, loopix_start_time, save_new_metrics_file).await?,
        };
        if *self != new_state {
            log::info!(
                "Node {} going to state {:?}",
                node.node_config.info.name,
                new_state
            );
        }

        Ok(new_state)
    }
}

impl From<LSRoot> for LoopixSimul {
    fn from(value: LSRoot) -> Self {
        LoopixSimul::Root(value)
    }
}

impl From<LSChild> for LoopixSimul {
    fn from(value: LSChild) -> Self {
        LoopixSimul::Child(value)
    }
}

// Root node actions
#[derive(Debug, PartialEq, Clone, Copy)]
enum LSRoot {
    // Wait for this many nodes to be online
    WaitNodes(usize, usize),
    // Wait for all nodes to report configuration
    WaitConfig(usize),
    // Sends a WebProx-request every 10 seconds
    SendProxyRequest(u32),
}

impl LSRoot {
    async fn process(&self, token: String, node: &mut Node, i: u32, retry: u8, duplicates: u8, start_time: Instant, loopix_start_time: u64, save_new_metrics_file: bool) -> Result<LoopixSimul, BrokerError> {
        match self {
            LSRoot::WaitNodes(n, n_clients) => {
                let path_len = *n;
                let nodes = (path_len * (path_len - 1)) + path_len + *n_clients;
                log::info!("Waiting for {} nodes", *n);
                let mut node_infos = node.nodes_online().unwrap();
                node_infos.insert(0, node.node_config.info.clone());
                log::info!("Found {} of {} nodes", node_infos.len(), nodes);
                if node_infos.len() == nodes {
                    let setup = LoopixSetup::new(path_len, node_infos, node.data_save_path.clone().unwrap(), *n_clients);
                    log::info!("Sending setup event");
                    node.gossip
                        .as_mut()
                        .unwrap()
                        .add_event(Event {
                            category: Category::LoopixSetup,
                            src: node.node_config.info.get_id(),
                            created: now(),
                            msg: serde_json::to_string(&setup).expect("serializing to string"),
                        })
                        .await?;
                    return Ok(LSRoot::WaitConfig(nodes).into());
                }
            }
            LSRoot::WaitConfig(_n) => {
                log::info!("Waiting for config");
                let node_configured = node
                    .gossip
                    .as_ref()
                    .unwrap()
                    .storage
                    .events(Category::LoopixConfig)
                    .len();
                log::info!("Child sees {} configured nodes", node_configured);
                LoopixSetup::node_config(node, duplicates, start_time, loopix_start_time).await?;
                let loopix_storage = node.loopix.as_ref().unwrap().storage.clone();
                let save_path = node.data_save_path.as_ref().unwrap();

                tokio::spawn(save_metrics_loop(
                    Arc::clone(&loopix_storage),
                    save_path.clone(),
                    save_new_metrics_file,
                    start_time,
                ));
                return Ok(LSRoot::SendProxyRequest(i + 10).into());
            }
            LSRoot::SendProxyRequest(_start) => {
                let timeout = if save_new_metrics_file {
                    6
                } else {
                    20
                };
                let start = now();
                let start_time = Instant::now();
                NUMBER_OF_PROXY_REQUESTS.inc();
                let timeout_duration = Duration::from_secs(timeout + 3);
                match tokio::time::timeout(timeout_duration, node
                    .webproxy
                    .as_mut()
                    .unwrap()
                    .get_with_retry_and_timeout(&format!("https://ipinfo.io?token={}", token), retry, Duration::from_secs(timeout))
                    // .get_with_retry_and_timeout_with_duplicates(&format!("https://ipinfo.io?token={}", token), retry, duplicates, Duration::from_secs(timeout))
                ).await {
                    Ok(Ok(mut res)) => match res.text().await {
                        Ok(body) => {
                            let end_to_end_time = start_time.elapsed().as_secs_f64();
                            END_TO_END_LATENCY.observe(end_to_end_time);
                            log::info!(
                                "---------------- Total time for request: {}ms",
                                now() - start
                            );
                            log::info!("---------------- Got reply from webproxy: {}", body);
                        }
                        Err(e) => log::info!("---------------- Couldn't get body: {e:?}"),
                    },
                    Ok(Err(e)) => log::info!("---------------- Webproxy returned error: {e:?}"),
                    Err(_) => log::info!("---------------- Request timed out"),
                }
                log::info!("---------------- Webproxy returned!");
            }
        }
        
        Ok((*self).into())
    }
}

// Child node actions
#[derive(Debug, PartialEq, Clone, Copy)]
enum LSChild {
    WaitConfig,
    ProxyRequesting(u32),
    ProxyReady,
}

impl LSChild {
    async fn process(&self, token: String, node: &mut Node, i: u32, retry: u8, duplicates: u8, start_time: Instant, loopix_start_time: u64, save_new_metrics_file: bool) -> Result<LoopixSimul, BrokerError> {
        match self {
            LSChild::WaitConfig => {
                log::info!("Child is waiting for config");
                let setup = LoopixSetup::node_config(node, duplicates, start_time, loopix_start_time).await?;
                if let Some(setup) = setup {
                    log::info!("{} got setup", node.node_config.info.name);

                    let loopix_storage = node.loopix.as_ref().unwrap().storage.clone();
                    let save_path = node.data_save_path.as_ref().unwrap();


                    tokio::spawn(save_metrics_loop(
                        Arc::clone(&loopix_storage),
                        save_path.clone(),
                        save_new_metrics_file,
                        start_time,
                    ));
                    if setup.role == LoopixRole::Client {
                        return Ok(LSChild::ProxyRequesting(i + 10).into());
                    } else {
                        return Ok(LSChild::ProxyReady.into());
                    }
                }
            } 

            LSChild::ProxyRequesting(_start) => {
                let timeout = if save_new_metrics_file {
                    6
                } else {
                    20
                };
                log::info!("Sending request through WebProxy at {} seconds since start with timeout {:?}", start_time.elapsed().as_secs_f64(), Duration::from_secs(timeout));
                let _start = now();
                let start_time = Instant::now();
                NUMBER_OF_PROXY_REQUESTS.inc();
                let start = now();
                let timeout_duration = Duration::from_secs(timeout);
                match tokio::time::timeout(timeout_duration, node
                    .webproxy
                    .as_mut()
                    .unwrap()
                    .get_with_retry_and_timeout(&format!("https://ipinfo.io?token={}", token), retry, Duration::from_secs(timeout))
                    // .get_with_retry_and_timeout_with_duplicates(&format!("https://ipinfo.io?token={}", token), retry, duplicates, Duration::from_secs(timeout))
                ).await {
                    Ok(Ok(mut res)) => match res.text().await {
                        Ok(body) => {
                            let end_to_end_time = start_time.elapsed().as_secs_f64();
                            END_TO_END_LATENCY.observe(end_to_end_time);
                            log::info!(
                                "---------------- Total time for request: {}ms",
                                now() - start
                            );
                            log::info!("---------------- Got reply from webproxy: {}", body);
                        }
                        Err(e) => log::info!("---------------- Couldn't get body: {e:?}"),
                    },
                    Ok(Err(e)) => log::info!("---------------- Webproxy returned error: {e:?}"),
                    Err(_) => log::info!("---------------- Request timed out"),
                }
                log::info!("---------------- Webproxy returning!");
                return Ok(LSChild::ProxyRequesting(i + 10).into());
            }
            LSChild::ProxyReady => {
                log::info!("Child is proxy ready");
            }

        }

        Ok((*self).into())
    }
}

// Modified LoopixSetup from flmodules/src/loopix/testing.rs to fit in the actual real
// running binaries.
// Instead of returning "LoopixNode"s, it returns the actual Brokers needed to store in
// the "Node".
// Also, this version takes a Vec<NodeInfo>, as they have been created by the binaries.
#[derive(Serialize, Deserialize)]
pub struct LoopixSetup {
    pub node_public_keys: HashMap<NodeID, PublicKey>,
    pub loopix_key_pairs: HashMap<NodeID, (PublicKey, StaticSecret)>,
    pub path_length: usize,
    pub all_nodes: Vec<NodeInfo>,
    pub config: String,
    pub role: LoopixRole,
    pub n_clients: usize,
}

impl LoopixSetup {
    pub async fn node_config(node: &mut Node, duplicates: u8, start_time: Instant, loopix_start_time: u64) -> Result<Option<LoopixSetup>, BrokerError> {
        // TODO case where loopix storage exists, so we're loading an already existign node
        log::info!("Current time since start: {}", start_time.elapsed().as_secs_f64());
        let events = node.gossip.as_ref().unwrap().events(Category::LoopixSetup);
        if let Some(event) = events.get(0) {
            // get event and setup info
            let mut setup = serde_json::from_str::<LoopixSetup>(&event.msg)
            .expect("deserializing loopix setup");
            let our_id = node.node_config.info.get_id();

            // let root know that node is ready
            node.gossip
            .as_mut()
            .unwrap()
            .add_event(Event {
                category: Category::LoopixConfig,
                src: node.node_config.info.get_id(),
                created: now(),
                msg: "Setup done".into(),
            })
            .await?;

            log::info!("NodeID {} is ready, waiting for loopix start time", node.node_config.info.get_id());

            // wait until scheduled start time
            while start_time.elapsed().as_secs() < loopix_start_time {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let (loopix, overlay) = setup.get_brokers(our_id, node.broker_net.clone(), duplicates).await?;

            log::info!("NodeID {} is ready, starting loopix", node.node_config.info.get_id());
            LOOPIX_START_TIME.set(start_time.elapsed().as_secs_f64());

            // start loopix and webproxy
            node.loopix = Some(loopix);
            node.webproxy = Some(
                WebProxy::start(
                    node.storage.clone(),
                    our_id,
                    overlay,
                    WebProxyConfig::default(),
                )
                .await
                .expect("Starting new WebProxy"),
            );
            
            return Ok(Some(setup));
        }
        Ok(None)
    }

    pub fn new(path_length: usize, all_nodes: Vec<NodeInfo>, config: String, n_clients: usize) -> Self {
        let (node_public_keys, loopix_key_pairs) = Self::create_nodes_and_keys(all_nodes.clone());
        log::info!("New loopix setup with {} nodes", all_nodes.len());
        Self {
            node_public_keys,
            loopix_key_pairs,
            path_length,
            all_nodes,
            config,
            role: LoopixRole::Client,
            n_clients,
        }
    }

    pub fn create_nodes_and_keys(
        all_nodes: Vec<NodeInfo>
    ) -> (
        HashMap<NodeID, PublicKey>,
        HashMap<NodeID, (PublicKey, StaticSecret)>,
    ) {
        let mut node_public_keys = HashMap::new();
        let mut loopix_key_pairs = HashMap::new();

        for node_info in all_nodes {
            let node_id = node_info.get_id();

            let (public_key, private_key) = LoopixStorage::generate_key_pair();
            node_public_keys.insert(node_id, public_key);
            loopix_key_pairs.insert(node_id, (public_key, private_key));
        }

        (node_public_keys, loopix_key_pairs)
    }

    pub async fn get_brokers(
        &mut self,
        node_id: NodeID,
        net: Broker<NetworkMessage>,
        duplicates: u8,
    ) -> Result<(LoopixBroker, Broker<OverlayMessage>), BrokerError> {
        let pos = self
            .all_nodes
            .iter()
            .position(|node| node.get_id() == node_id)
            .expect("node_id in list");
        log::info!("Node list: {:?}", self.all_nodes);
        log::info!("NodeID {} is at position {}", node_id, pos);
        let role = if pos < self.n_clients {
            log::info!("Role: Client");
            LoopixRole::Client
        } else if pos < self.n_clients + self.path_length {
            log::info!("Role: Provider");
            LoopixRole::Provider
        } else {
            log::info!("Role: Mixnode");
            LoopixRole::Mixnode
        };

        self.role = role.clone();
        let config = self.get_config(node_id, role).await?;
        let loopix_broker = LoopixBroker::start(net, config, duplicates).await?;
        let overlay = OverlayLoopix::start(loopix_broker.broker.clone()).await?;

        wait_ms(3000).await;
        Ok((loopix_broker, overlay))
    }

    pub async fn get_config(
        &self,
        node_id: NodeID,
        role: LoopixRole,
    ) -> Result<LoopixConfig, BrokerError> {
        let private_key = &self.loopix_key_pairs.get(&node_id).unwrap().1;
        let public_key = &self.loopix_key_pairs.get(&node_id).unwrap().0;

        let mut config_path = PathBuf::from(&self.config);
        config_path.push("loopix_core_config.yaml");

        let config_str = std::fs::read_to_string(config_path.clone()).unwrap();
        let core_config: CoreConfig = serde_yaml::from_str(&config_str).unwrap();

        // "loopix_storage.yaml" is in self.config directory load it into memory
        let mut storage_path = PathBuf::from(&self.config);
        storage_path.push("loopix_storage.yaml");

        let config;
        if storage_path.exists() {
            log::info!("Storage path: {:?}", storage_path);
            let storage_str = std::fs::read_to_string(storage_path.clone()).unwrap();
            let storage: LoopixStorage = serde_yaml::from_str(&storage_str).unwrap();
            log::info!("Storage: {}", storage_str);
            config = LoopixConfig::new(role, storage, core_config).await;
        } else {
            config = LoopixConfig::default_with_path_length_and_n_clients(
                role,
                node_id,
                self.path_length as usize,
                self.n_clients,
                private_key.clone(),
                public_key.clone(),
                self.all_nodes.clone(),
                core_config,
            );
        }

        config
            .storage_config
            .set_node_public_keys(self.node_public_keys.clone())
            .await;

        Ok(config)
    }
}
