use std::collections::HashMap;

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
        config::{LoopixConfig, LoopixRole},
        messages::LoopixMessage,
        storage::LoopixStorage,
    },
    network::{messages::NetworkMessage, network_broker_start, signal::SIGNAL_VERSION},
    nodeconfig::NodeInfo,
    overlay::{broker::loopix::OverlayLoopix, messages::OverlayMessage},
    web_proxy::{broker::WebProxy, core::WebProxyConfig},
};
use flnode::{node::Node, version::VERSION_STRING};
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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

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
    let mut node = Node::start(Box::new(storage), node_config, network).await?;
    let nc = node.node_config.info.clone();
    let mut state = match args.path_len {
        Some(len) => LoopixSimul::Root(LSRoot::WaitNodes((len + 2) * len)),
        None => LoopixSimul::Child(LSChild::WaitConfig),
    };
    log::info!(
        "Starting node with state {:?} {}: {}",
        state,
        nc.get_id(),
        nc.name
    );

    log::info!("Started successfully");
    let mut i: i32 = 0;
    loop {
        i += 1;
        node.process()
            .await
            .err()
            .map(|e| log::warn!("Couldn't process node: {e:?}"));

        state.process(&mut node).await?;

        if i % 3 == 2 {
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
    async fn process(&mut self, node: &mut Node) -> Result<(), BrokerError> {
        let old_state = *self;
        match &self {
            LoopixSimul::Root(mut lsroot) => lsroot.process(node).await?,
            LoopixSimul::Child(mut lschild) => lschild.process(node).await?,
        }
        if *self != old_state {
            log::info!(
                "Node {} going to state {:?}",
                node.node_config.info.name,
                self
            );
        }

        Ok(())
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
    SendProxyRequest,
}

impl LSRoot {
    async fn process(&mut self, node: &mut Node) -> Result<(), BrokerError> {
        match self {
            LSRoot::WaitNodes(n) => {
                let path_len = *n;
                let node_infos = node.nodes_online().unwrap();
                if node_infos.len() == path_len {
                    *self = LSRoot::WaitConfig(path_len);
                    let setup = LoopixSetup::new(path_len, node_infos);
                    node.gossip
                        .as_mut()
                        .unwrap()
                        .add_event(Event {
                            category: Category::TextMessage,
                            src: node.node_config.info.get_id(),
                            created: now(),
                            msg: serde_json::to_string(&setup).expect("serializing to string"),
                        })
                        .await?;
                }
            }
            LSRoot::WaitConfig(n) => {
                if node
                    .gossip
                    .as_ref()
                    .unwrap()
                    .storage
                    .events(Category::TextMessage)
                    .len()
                    == *n + 1
                {}
            }
            LSRoot::SendProxyRequest => todo!(),
        }
        Ok(())
    }
}

// Child node actions
#[derive(Debug, PartialEq, Clone, Copy)]
enum LSChild {
    WaitConfig,
    ProxyReady,
}

impl LSChild {
    async fn process(&mut self, node: &mut Node) -> Result<(), BrokerError> {
        match self {
            LSChild::WaitConfig => {
                let events = node.gossip.as_ref().unwrap().events(Category::TextMessage);
                if let Some(event) = events.get(0) {
                    let setup = serde_json::from_str::<LoopixSetup>(&event.msg)
                        .expect("deserializing loopix setup");
                    let our_id = node.node_config.info.get_id();
                    let (loopix, overlay) =
                        setup.get_brokers(our_id, node.broker_net.clone()).await?;
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
            }
            LSChild::ProxyReady => todo!(),
        }

        Ok(())
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
    ) -> Result<(Broker<LoopixMessage>, Broker<OverlayMessage>), BrokerError> {
        let pos = self
            .all_nodes
            .iter()
            .position(|node| node.get_id() == node_id)
            .expect("node_id in list");
        let role = if pos < self.path_length {
            LoopixRole::Client
        } else if pos < (self.path_length + 1) * self.path_length {
            LoopixRole::Mixnode
        } else {
            LoopixRole::Provider
        };
        let config = self.get_config(node_id, role).await?;
        let loopix_broker = LoopixBroker::start(net, config).await?;
        Ok((
            loopix_broker.clone(),
            OverlayLoopix::start(loopix_broker).await?,
        ))
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

        config
            .storage_config
            .set_node_public_keys(self.node_public_keys.clone())
            .await;

        Ok(config)
    }
}
