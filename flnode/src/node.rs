use std::collections::HashMap;

use log::{error, info};
use thiserror::Error;

use flarch::{
    data_storage::{DataStorage, StorageError},
    tasks::now,
};
use flmodules::{
    broker::{Broker, BrokerError},
    gossip_events::{
        broker::GossipBroker,
        events::{self, Category, Event},
        module::{GossipIn, GossipMessage},
    },
    nodeids::NodeID,
    ping::{broker::PingBroker, module::PingConfig},
    timer::{BrokerTimer, TimerMessage},
};
use flnet::{
    config::{ConfigError, NodeConfig, NodeInfo},
    network::{NetCall, NetworkError, NetworkMessage},
};

use crate::modules::{random::RandomBroker, stat::StatBroker};

#[derive(Error, Debug)]
pub enum NodeError {
    #[error("Couldn't get lock")]
    Lock,
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
}

use bitflags::bitflags;
bitflags! {
    pub struct Brokers: u32 {
        const ENABLE_STAT = 0b1;
        const ENABLE_RAND = 0b10;
        const ENABLE_GOSSIP = 0b100;
        const ENABLE_PING = 0b1000;
        const ENABLE_ALL = 0b1111;
    }
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
    pub stat: StatBroker,
    /// Handles a random number of connections
    pub random: RandomBroker,
    /// Gossip-events sent and received
    pub gossip: GossipBroker,
    /// Pings all connected nodes and informs about failing nodes
    pub ping: PingBroker,
}

const STORAGE_GOSSIP_EVENTS: &str = "gossip_events";
const STORAGE_CONFIG: &str = "nodeConfig";

impl Node {
    /// Create new node by loading the config from the storage.
    /// This also initializes the network and starts listening for
    /// new messages from the signalling server and from other nodes.
    /// The actual logic is handled in Logic.
    pub async fn start(
        storage: Box<dyn DataStorage>,
        node_config: NodeConfig,
        broker_net: Broker<NetworkMessage>,
    ) -> Result<Self, NodeError> {
        let mut node =
            Self::start_some(storage, node_config, broker_net, Brokers::ENABLE_ALL).await?;
        node.add_timer(BrokerTimer::start().await?).await;
        Ok(node)
    }

    /// Same as `start`, but only initialize a subset of brokers.
    pub async fn start_some(
        storage: Box<dyn DataStorage>,
        node_config: NodeConfig,
        broker_net: Broker<NetworkMessage>,
        brokers: Brokers,
    ) -> Result<Self, NodeError> {
        info!(
            "Starting node: {} = {}",
            node_config.info.name,
            node_config.info.get_id()
        );

        let id = node_config.info.get_id();
        let mut nd = Self {
            storage,
            node_config,
            broker_net,
            stat: StatBroker::start(Broker::new()).await?,
            random: RandomBroker::start(id, Broker::new()).await?,
            gossip: GossipBroker::start(id, Broker::new()).await?,
            ping: PingBroker::start(PingConfig::default(), Broker::new()).await?,
        };
        if brokers.contains(Brokers::ENABLE_RAND) {
            nd.random = RandomBroker::start(id, nd.broker_net.clone()).await?;
            if brokers.contains(Brokers::ENABLE_GOSSIP) {
                nd.gossip = GossipBroker::start(id, nd.random.broker.clone()).await?;
                Self::init_gossip(&mut nd.gossip, &nd.storage, &nd.node_config.info).await?;
            }
            if brokers.contains(Brokers::ENABLE_PING) {
                nd.ping =
                    PingBroker::start(PingConfig::default(), nd.random.broker.clone()).await?;
            }
        }
        if brokers.contains(Brokers::ENABLE_STAT) {
            nd.stat = StatBroker::start(nd.broker_net.clone()).await?;
        }

        Ok(nd)
    }

    /// Adds a timer broker to the Node. Automatically called by Node::start.
    pub async fn add_timer(&mut self, mut timer: Broker<TimerMessage>) {
        timer
            .forward(
                self.broker_net.clone(),
                Box::new(|msg| (msg == TimerMessage::Second).then(|| NetCall::Tick.into())),
            )
            .await;
        self.random.add_timer(timer.clone()).await;
        self.gossip.add_timer(timer.clone()).await;
        self.ping.add_timer(timer).await;
    }

    /// Update all data-storage. Goes through all storage modules, reads the queues of messages,
    /// and processes the ones with updated data.
    pub fn update(&mut self) {
        self.stat.update();
        self.random.update();
        self.gossip.update();
        self.ping.update();
    }

    /// Start processing of network and logic messages, in case they haven't been
    /// called automatically.
    /// Also updates all storage fields in the node_data field.
    pub async fn process(&mut self) -> Result<(), NodeError> {
        self.update();
        self.storage
            .set(STORAGE_GOSSIP_EVENTS, &self.gossip.storage.get()?)?;
        Ok(())
    }

    /// Requests a list of all connected nodes
    pub async fn request_list(&mut self) -> Result<(), NodeError> {
        self.broker_net
            .emit_msg(NetCall::SendWSUpdateListRequest.into())
            .await?;
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
        self.nodes_info(self.random.storage.connected.get_nodes().0)
    }

    /// Returns all currently online nodes in the whole system. Every node will only connect
    /// to a subset of these nodes, which can be get with `nodes_connected`.
    pub fn nodes_online(&self) -> Result<Vec<NodeInfo>, NodeError> {
        self.nodes_info(self.random.storage.known.0.clone())
    }

    /// Returns a list of known nodes from the local storage
    pub fn nodes_info_all(&self) -> Result<HashMap<NodeID, NodeInfo>, NodeError> {
        let events = self.gossip.events(Category::NodeInfo);

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
    }

    /// Adds a new chat message that will be broadcasted to the system.
    pub async fn add_chat_message(&mut self, msg: String) -> Result<(), NodeError> {
        let event = events::Event {
            category: events::Category::TextMessage,
            src: self.node_config.info.get_id(),
            created: now(),
            msg,
        };
        self.gossip.add_event(event).await?;
        Ok(())
    }

    // Reads the gossip configuration and stores it in the gossip-storage.
    async fn init_gossip(
        gossip: &mut GossipBroker,
        gossip_storage: &Box<dyn DataStorage>,
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
            )))
            .await?;
        Ok(())
    }

    /// Static method

    /// Fetches the config
    pub fn get_config(storage: Box<dyn DataStorage>) -> Result<NodeConfig, NodeError> {
        let client = "unknown";

        let config_str = match storage.get(STORAGE_CONFIG) {
            Ok(s) => s,
            Err(_) => {
                log::info!("Couldn't load configuration - start with empty");
                "".to_string()
            }
        };
        let mut config = NodeConfig::decode(&config_str)?;
        config.info.client = client.to_string();
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
    use flarch::{data_storage::TempDS, start_logging};
    use flmodules::gossip_events::{
        events::{Category, Event},
        module::GossipIn,
    };

    use super::*;

    #[tokio::test]
    async fn test_storage() -> Result<(), Box<dyn std::error::Error>> {
        start_logging();

        let storage = TempDS::new();
        let nc = NodeConfig::new();
        let mut nd = Node::start(storage.clone(), nc.clone(), Broker::new()).await?;
        let event = Event {
            category: Category::TextMessage,
            src: nc.info.get_id(),
            created: 0.,
            msg: "something".into(),
        };
        nd.gossip
            .broker
            .emit_msg(GossipIn::AddEvent(event.clone()).into())
            .await?;
        nd.process().await?;

        let nd2 = Node::start(storage.clone(), nc.clone(), Broker::new()).await?;
        let events = nd2.gossip.storage.events(Category::TextMessage);
        assert_eq!(1, events.len());
        assert_eq!(&event, events.get(0).unwrap());
        Ok(())
    }

    #[tokio::test]
    async fn test_store_node() -> Result<(), Box<dyn std::error::Error>> {
        start_logging();

        let mut node = Node::start(TempDS::new(), NodeConfig::new(), Broker::new()).await?;
        node.update();
        log::debug!("storage is: {:?}", node.gossip.storage);
        assert_eq!(1, node.gossip.events(Category::NodeInfo).len());
        Ok(())
    }
}
