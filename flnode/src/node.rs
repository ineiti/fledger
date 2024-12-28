use log::{error, info};
use std::collections::HashMap;
use thiserror::Error;

use flarch::{
    broker::{Broker, BrokerError},
    nodeids::NodeID,
};
use flarch::{
    data_storage::{DataStorage, StorageError},
    tasks::now,
};
use flmodules::{
    gossip_events::{
        broker::GossipBroker,
        core::{self, Category, Event},
        messages::{GossipIn, GossipMessage},
    },
    loopix::broker::LoopixBroker,
    network::messages::{NetworkError, NetworkIn, NetworkMessage},
    nodeconfig::{ConfigError, NodeConfig, NodeInfo},
    overlay::broker::{direct::OverlayDirect, random::OverlayRandom},
    ping::{broker::PingBroker, messages::PingConfig},
    random_connections::broker::RandomBroker,
    timer::{TimerBroker, TimerMessage},
    web_proxy::{
        broker::{WebProxy, WebProxyError},
        core::WebProxyConfig,
    },
    Modules,
};

use crate::stat::StatBroker;

#[derive(Error, Debug)]
pub enum NodeError {
    #[error("Couldn't get lock")]
    Lock,
    #[error("Missing subsystem {0}")]
    Missing(String),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Network(#[from] NetworkError),
    #[error(transparent)]
    Broker(#[from] BrokerError),
    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),
    #[error(transparent)]
    WebProxy(#[from] WebProxyError),
}

/// The node structure holds it all together. It is the main structure of the project.
pub struct Node {
    /// The node configuration
    pub node_config: NodeConfig,
    /// Storage to be used
    pub storage: Box<dyn DataStorage>,
    /// Network broker
    pub broker_net: Broker<NetworkMessage>,

    // Subsystem data
    /// Stores the connection data
    pub stat: Option<StatBroker>,
    /// Handles a random number of connections
    pub random: Option<RandomBroker>,
    /// Gossip-events sent and received
    pub gossip: Option<GossipBroker>,
    /// Pings all connected nodes and informs about failing nodes
    pub ping: Option<PingBroker>,
    /// Answers GET requests from another node
    pub webproxy: Option<WebProxy>,
    /// Create a mix-network for anonymous communication
    pub loopix: Option<LoopixBroker>,

    /// Data save path
    pub data_save_path: Option<String>,
}

const STORAGE_GOSSIP_EVENTS: &str = "gossip_events";
const STORAGE_CONFIG: &str = "nodeConfig";

impl Node {
    /// Create new node by loading the config from the storage.
    /// This also initializes the network and starts listening for
    /// new messages from the signalling server and from other nodes.
    /// The actual logic is handled in Logic.
    pub async fn start(
        storage: Box<dyn DataStorage + Send>,
        node_config: NodeConfig,
        broker_net: Broker<NetworkMessage>,
        data_save_path: Option<String>,
    ) -> Result<Self, NodeError> {
        info!(
            "Starting node: {} = {}",
            node_config.info.name,
            node_config.info.get_id()
        );

        let modules = node_config.info.modules;
        let id = node_config.info.get_id();
        let mut random = None;
        let mut gossip = None;
        let mut ping = None;
        let loopix = None;
        let mut overlay = OverlayDirect::start(broker_net.clone()).await?;
        if modules.contains(Modules::ENABLE_RAND) {
            let rnd = RandomBroker::start(id, broker_net.clone()).await?;
            if modules.contains(Modules::ENABLE_GOSSIP) {
                gossip = Some(GossipBroker::start(id, rnd.broker.clone()).await?);
                Self::init_gossip(
                    &mut gossip.as_mut().unwrap(),
                    storage.clone(),
                    &node_config.info,
                )
                .await?;
            }
            if modules.contains(Modules::ENABLE_PING) {
                ping = Some(PingBroker::start(PingConfig::default(), rnd.broker.clone()).await?);
            }
            overlay = OverlayRandom::start(rnd.broker.clone()).await?;
            random = Some(rnd);
        }
        // This needs the `config`, which is not availble yet.
        // if modules.contains(Modules::ENABLE_LOOPIX) {
        //     loopix = Some(LoopixBroker::start(Broker::new(), broker_net.clone(), config).await?);
        //     overlay = OverlayLoopix::start(loopix.unwrap().clone()).await?;
        // }
        let webproxy = if modules.contains(Modules::ENABLE_WEBPROXY) {
            Some(
                WebProxy::start(
                    storage.clone(),
                    id,
                    overlay.clone(),
                    WebProxyConfig::default(),
                )
                .await?,
            )
        } else {
            None
        };
        let stat = if modules.contains(Modules::ENABLE_STAT) {
            Some(StatBroker::start(broker_net.clone()).await?)
        } else {
            None
        };

        let mut node = Self {
            storage,
            node_config,
            broker_net,
            stat,
            random,
            gossip,
            ping,
            webproxy,
            loopix,
            data_save_path,
        };
        node.add_timer(TimerBroker::start().await?).await;
        Ok(node)
    }

    /// Adds a timer broker to the Node. Automatically called by Node::start.
    pub async fn add_timer(&mut self, mut timer: Broker<TimerMessage>) {
        timer
            .forward(
                self.broker_net.clone(),
                Box::new(|msg| (msg == TimerMessage::Second).then(|| NetworkIn::Tick.into())),
            )
            .await;
        if let Some(r) = self.random.as_mut() {
            r.add_timer(timer.clone()).await;
        }
        if let Some(g) = self.gossip.as_mut() {
            g.add_timer(timer.clone()).await;
        }
        if let Some(p) = self.ping.as_mut() {
            p.add_timer(timer).await;
        }
    }

    /// Update all data-storage. Goes through all storage modules, reads the queues of messages,
    /// and processes the ones with updated data.
    pub fn update(&mut self) {
        if let Some(s) = self.stat.as_mut() {
            s.update();
        }
        if let Some(r) = self.random.as_mut() {
            r.update();
        }
        if let Some(g) = self.gossip.as_mut() {
            g.update();
        }
        if let Some(p) = self.ping.as_mut() {
            p.update();
        }
    }

    /// Start processing of network and logic messages, in case they haven't been
    /// called automatically.
    /// Also updates all storage fields in the node_data field.
    pub async fn process(&mut self) -> Result<(), NodeError> {
        self.update();
        if let Some(g) = self.gossip.as_mut() {
            self.storage.set(STORAGE_GOSSIP_EVENTS, &g.storage.get()?)?;
        }
        Ok(())
    }

    /// Requests a list of all connected nodes
    pub async fn request_list(&mut self) -> Result<(), NodeError> {
        self.broker_net
            .emit_msg(NetworkIn::WSUpdateListRequest.into())?;
        Ok(())
    }

    /// Returns all NodeInfos that are stored locally. All ids that do not have a
    /// corresponding NodeInfo in the local storage are dropped.
    pub fn nodes_info(&self, ids: Vec<NodeID>) -> Result<Vec<NodeInfo>, NodeError> {
        let mut nodeinfos = self.nodes_info_all()?;
        Ok(ids.iter().filter_map(|id| nodeinfos.remove(&id)).collect())
    }

    /// Gets the current list of connected nodes - these are the nodes that this node is
    /// currently connected to, and can be shorter than the list of all nodes in the system.
    pub fn nodes_connected(&self) -> Result<Vec<NodeInfo>, NodeError> {
        if let Some(r) = self.random.as_ref() {
            return self.nodes_info(r.storage.connected.get_nodes().0);
        }
        Err(NodeError::Missing("Random".into()))
    }

    /// Returns all currently online nodes in the whole system. Every node will only connect
    /// to a subset of these nodes, which can be get with `nodes_connected`.
    pub fn nodes_online(&self) -> Result<Vec<NodeInfo>, NodeError> {
        if let Some(r) = self.random.as_ref() {
            return self.nodes_info(r.storage.known.0.clone());
        }
        Err(NodeError::Missing("Random".into()))
    }

    /// Returns a list of known nodes from the local storage
    pub fn nodes_info_all(&self) -> Result<HashMap<NodeID, NodeInfo>, NodeError> {
        if let Some(g) = self.gossip.as_ref() {
            let events = g.events(Category::NodeInfo);

            let mut nodeinfos = HashMap::new();
            for ni in events {
                // For some reason I cannot get it to work the from_str in a .iter().map()
                match NodeInfo::decode(&ni.msg) {
                    Ok(info) => {
                        nodeinfos.insert(info.get_id(), info);
                    }
                    Err(e) => log::error!("Parse-error {e:?} for {}", ni.msg),
                }
            }
            Ok(nodeinfos)
        } else {
            Err(NodeError::Missing("Gossip".into()))
        }
    }

    /// Adds a new chat message that will be broadcasted to the system.
    pub async fn add_chat_message(&mut self, msg: String) -> Result<(), NodeError> {
        if let Some(g) = self.gossip.as_mut() {
            let event = core::Event {
                category: core::Category::TextMessage,
                src: self.node_config.info.get_id(),
                created: now(),
                msg,
            };
            g.add_event(event).await?;
            Ok(())
        } else {
            Err(NodeError::Missing("Gossip".into()))
        }
    }

    // Reads the gossip configuration and stores it in the gossip-storage.
    async fn init_gossip(
        gossip: &mut GossipBroker,
        gossip_storage: Box<dyn DataStorage>,
        node_info: &NodeInfo,
    ) -> Result<(), NodeError> {
        let gossip_msgs_str = gossip_storage.get(STORAGE_GOSSIP_EVENTS).unwrap();
        if !gossip_msgs_str.is_empty() {
            if let Err(e) = gossip.storage.set(&gossip_msgs_str) {
                log::warn!("Couldn't load gossip messages: {}", e);
            }
        }
        gossip.storage.add_event(Event {
            category: Category::NodeInfo,
            src: node_info.get_id(),
            created: now(),
            msg: node_info.encode(),
        });
        gossip
            .broker
            .emit_msg(GossipMessage::Input(GossipIn::SetStorage(
                gossip.storage.clone(),
            )))?;
        Ok(())
    }

    /// Static method

    /// Fetches the config
    pub fn get_config(storage: Box<dyn DataStorage>) -> Result<NodeConfig, NodeError> {
        log::info!("Getting config from {}", STORAGE_CONFIG);
        let config_str = match storage.get(STORAGE_CONFIG) {
            Ok(s) => {
                log::info!("Loaded configuration: {}", s);
                s
            },
            Err(_) => {
                log::info!("Couldn't load configuration - start with empty");
                "".to_string()
            }
        };
        log::info!("Here is the config: {}", config_str);
        let mut config = NodeConfig::decode(&config_str)?;
        #[cfg(target_family = "wasm")]
        let enable_webproxy_request = false;
        // Only unix based clients can send http GET requests.
        #[cfg(target_family = "unix")]
        let enable_webproxy_request = true;

        config
            .info
            .modules
            .set(Modules::ENABLE_WEBPROXY_REQUESTS, enable_webproxy_request);
        Self::set_config(storage, &config.encode())?;
        Ok(config)
    }

    /// Updates the config of the node
    pub fn set_config(mut storage: Box<dyn DataStorage>, config: &str) -> Result<(), NodeError> {
        storage.set(STORAGE_CONFIG, config)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use flarch::{data_storage::DataStorageTemp, start_logging};
    use flmodules::gossip_events::{
        core::{Category, Event},
        messages::GossipIn,
    };

    use super::*;

    #[tokio::test]
    async fn test_storage() -> Result<(), Box<dyn std::error::Error>> {
        start_logging();

        let storage = DataStorageTemp::new();
        let nc = NodeConfig::new();
        let mut nd = Node::start(storage.clone(), nc.clone(), Broker::new(), None).await?;
        let event = Event {
            category: Category::TextMessage,
            src: nc.info.get_id(),
            created: 0,
            msg: "something".into(),
        };
        nd.gossip
            .as_mut()
            .unwrap()
            .broker
            .settle_msg(GossipIn::AddEvent(event.clone()).into())
            .await?;
        nd.process().await?;

        let nd2 = Node::start(storage.clone(), nc.clone(), Broker::new(), None  ).await?;
        let events = nd2.gossip.unwrap().storage.events(Category::TextMessage);
        assert_eq!(1, events.len());
        assert_eq!(&event, events.get(0).unwrap());
        Ok(())
    }

    #[tokio::test]
    async fn test_store_node() -> Result<(), Box<dyn std::error::Error>> {
        start_logging();

        let mut node = Node::start(
            Box::new(DataStorageTemp::new()),
            NodeConfig::new(),
            Broker::new(),
            None,
        )
        .await?;
        node.update();
        log::debug!("storage is: {:?}", node.gossip.as_ref().unwrap().storage);
        assert_eq!(1, node.gossip.unwrap().events(Category::NodeInfo).len());
        Ok(())
    }
}
