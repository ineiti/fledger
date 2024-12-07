use std::{collections::HashMap, fs::File, io::Write, path::PathBuf, sync::Arc, time::Duration};

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
        broker::LoopixBroker,
        config::{CoreConfig, LoopixConfig, LoopixRole},
        storage::LoopixStorage,
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

    /// Uptime interval - to stress test disconnections
    #[clap(short, long)]
    path_len: Option<usize>,

    /// Reliability config
    #[clap(long, default_value_t = 0)]
    retry: u8,
}

async fn save_metrics_loop(
    storage: Arc<LoopixStorage>,
    dir_path: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let path = PathBuf::from(dir_path);

    let mut storage_path = PathBuf::from(".");
    storage_path.push(path.clone());
    if let Err(e) = std::fs::create_dir_all(&storage_path) {
        log::error!("Failed to create directory: {}", e);
    }
    storage_path.push("loopix_storage.yaml");

    let mut metrics_path = PathBuf::from(".");
    metrics_path.push(path);
    if let Err(e) = std::fs::create_dir_all(&metrics_path) {
        log::error!("Failed to create directory: {}", e);
    }
    metrics_path.push("metrics.txt");

    loop {
        std::thread::sleep(Duration::from_secs(5));

        // Save the loopix storage to a file
        let storage_bytes = storage.to_yaml_async().await.unwrap();
        let mut file = File::create(&storage_path).expect("Unable to create file");
        file.write_all(storage_bytes.as_bytes())
            .expect("Unable to write data");

        // Gather the current metrics
        let metric_families = gather();
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();

        // Write metrics to a file
        let mut file = File::create(&metrics_path).expect("Unable to create file");
        file.write_all(&buffer).expect("Unable to write data");
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let config = args.config.clone();

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
    let mut node = Node::start(Box::new(storage), node_config, network, Some(config)).await?;
    let nc = node.node_config.info.clone();
    let mut state = match args.path_len {
        Some(len) => LoopixSimul::Root(LSRoot::WaitNodes(len)),
        None => LoopixSimul::Child(LSChild::WaitConfig),
    };
    log::info!(
        "Starting node with state {:?} {}: {}",
        state,
        nc.get_id(),
        nc.name
    );

    log::info!("Started successfully");
    let mut i: u32 = 0;
    loop {
        i += 1;
        node.process()
            .await
            .err()
            .map(|e| log::warn!("Couldn't process node: {e:?}"));

        state = state.process(&mut node, i).await?;

        if i % 3 == 2 && false {
            log::info!("Nodes are: {:?}", node.nodes_online()?);
            let ping = &node.ping.as_ref().unwrap().storage;
            log::info!("Nodes countdowns are: {:?}", ping.stats);
            log::debug!(
                "Chat messages are: {:?}",
                node.gossip.as_ref().unwrap().chat_events()
            );
        }
        wait_ms(1000).await;
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
    async fn process(&self, node: &mut Node, i: u32) -> Result<Self, BrokerError> {
        let new_state = match &self {
            LoopixSimul::Root(lsroot) => lsroot.process(node, i).await?,
            LoopixSimul::Child(lschild) => lschild.process(node).await?,
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
    WaitNodes(usize),
    // Wait for all nodes to report configuration
    WaitConfig(usize),
    // Sends a WebProx-request every 10 seconds
    SendProxyRequest(u32),
}

impl LSRoot {
    async fn process(&self, node: &mut Node, i: u32) -> Result<LoopixSimul, BrokerError> {
        match self {
            LSRoot::WaitNodes(n) => {
                let path_len = *n;
                let nodes = (path_len * path_len) + path_len * 2;
                let mut node_infos = node.nodes_online().unwrap();
                node_infos.insert(0, node.node_config.info.clone());
                log::info!("Found {} of {} nodes", node_infos.len(), nodes);
                if node_infos.len() == nodes {
                    let setup = LoopixSetup::new(path_len, node_infos);
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
            LSRoot::WaitConfig(n) => {
                let node_configured = node
                    .gossip
                    .as_ref()
                    .unwrap()
                    .storage
                    .events(Category::LoopixConfig)
                    .len();
                log::info!("Root sees {} configured nodes", node_configured);
                if node_configured + 1 == *n {
                    LoopixSetup::node_config(node).await?;
                    let loopix_storage = node.loopix.as_ref().unwrap().storage.clone();
                    let save_path = node.data_save_path.as_ref().unwrap();
                    tokio::spawn(save_metrics_loop(
                        Arc::clone(&loopix_storage),
                        save_path.clone(),
                    ));
                    return Ok(LSRoot::SendProxyRequest(i + 10).into());
                }
            }
            LSRoot::SendProxyRequest(start) => {
                if (i - *start) % 10 == 0 {
                    log::info!("Sending request through WebProxy");
                    let start = now();
                    match node
                        .webproxy
                        .as_mut()
                        .unwrap()
                        .get_with_timeout("https://ipinfo.io", Duration::from_secs(60))
                        .await
                    {
                        Ok(mut res) => match res.text().await {
                            Ok(body) => {
                                log::info!(
                                    "---------------- Total time for request: {}ms",
                                    now() - start
                                );
                                log::info!("---------------- Got reply from webproxy: {}", body);
                            }
                            Err(e) => log::info!("---------------- Couldn't get body: {e:?}"),
                        },
                        Err(e) => log::info!("---------------- Webproxy returned error: {e:?}"),
                    }
                }
            }
        }
        Ok((*self).into())
    }
}

// Child node actions
#[derive(Debug, PartialEq, Clone, Copy)]
enum LSChild {
    WaitConfig,
    ProxyReady,
}

impl LSChild {
    async fn process(&self, node: &mut Node) -> Result<LoopixSimul, BrokerError> {
        match self {
            LSChild::WaitConfig => {
                if LoopixSetup::node_config(node).await? {
                    log::info!("{} got setup", node.node_config.info.name);
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

                    let loopix_storage = node.loopix.as_ref().unwrap().storage.clone();
                    let save_path = node.data_save_path.as_ref().unwrap();
                    tokio::spawn(save_metrics_loop(
                        Arc::clone(&loopix_storage),
                        save_path.clone(),
                    ));
                    return Ok(LSChild::ProxyReady.into());
                }
            }
            LSChild::ProxyReady => {}
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
}

impl LoopixSetup {
    pub async fn node_config(node: &mut Node) -> Result<bool, BrokerError> {
        let events = node.gossip.as_ref().unwrap().events(Category::LoopixSetup);
        if let Some(event) = events.get(0) {
            // DEBUG: if you set this to 'false', loopix will not be setup, and you'll just see
            // how the rest of the system sets up.
            if true {
                let setup = serde_json::from_str::<LoopixSetup>(&event.msg)
                    .expect("deserializing loopix setup");
                let our_id = node.node_config.info.get_id();
                let (loopix, overlay) = setup.get_brokers(our_id, node.broker_net.clone()).await?;
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
            }
            return Ok(true);
        }
        Ok(false)
    }

    pub fn new(path_length: usize, all_nodes: Vec<NodeInfo>) -> Self {
        let (node_public_keys, loopix_key_pairs) = Self::create_nodes_and_keys(all_nodes.clone());

        Self {
            node_public_keys,
            loopix_key_pairs,
            path_length,
            all_nodes,
        }
    }

    pub fn create_nodes_and_keys(
        all_nodes: Vec<NodeInfo>,
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
        &self,
        node_id: NodeID,
        net: Broker<NetworkMessage>,
    ) -> Result<(LoopixBroker, Broker<OverlayMessage>), BrokerError> {
        let pos = self
            .all_nodes
            .iter()
            .position(|node| node.get_id() == node_id)
            .expect("node_id in list");
        let role = if pos < self.path_length {
            LoopixRole::Client
        } else if pos < self.path_length * 2 {
            LoopixRole::Provider
        } else {
            LoopixRole::Mixnode
        };

        let config = self.get_config(node_id, role).await?;
        let loopix_broker = LoopixBroker::start(net, config).await?;
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

        let config = LoopixConfig::default_with_path_length(
            role,
            node_id,
            self.path_length as usize,
            private_key.clone(),
            public_key.clone(),
            self.all_nodes.clone(),
        );

        // let config_path = PathBuf::from("./loopix_core_config.yaml");

        // let config_str = std::fs::read_to_string(config_path.clone()).unwrap();

        // let core_config: CoreConfig = serde_yaml::from_str(&config_str).unwrap();

        // let config = LoopixConfig::default_with_core_config_and_path_length(
        //     role,
        //     node_id,
        //     self.path_length as usize,
        //     private_key.clone(),
        //     public_key.clone(),
        //     self.all_nodes.clone(),
        //     core_config,
        // );

        config
            .storage_config
            .set_node_public_keys(self.node_public_keys.clone())
            .await;

        Ok(config)
    }
}
