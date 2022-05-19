use std::collections::HashMap;

use flnet::{network::{NetworkMessage, NetworkConnectionState}, signal::websocket::WSClientMessage, config::NodeConfig};
use log::{error, info};
use thiserror::Error;

use flmodules::{
    gossip_events::events::{self, Category},
    ping::storage::PingStorage,
};
use flnet::{
    config::{ConfigError, NodeInfo},
    network::{NetCall, Network, NetworkError},
    signal::web_rtc::WebRTCSpawner,
};
use flutils::{
    broker::{Broker, BrokerError},
    data_storage::{DataStorage, DataStorageBase, StorageError},
    nodeids::{NodeID, U256},
    arch::now,
};

use crate::{
    modules::timer::BrokerTimer,
    node_data::{NodeData, NodeDataError},
};

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
    #[error(transparent)]
    NodeData(#[from] NodeDataError),
}

/// The node structure holds it all together. It is the main structure of the project.
pub struct Node {
    node_data: NodeData,
    broker_net: Broker<NetworkMessage>,
}

pub const CONFIG_NAME: &str = "nodeConfig";

impl Node {
    /// Create new node by loading the config from the storage.
    /// This also initializes the network and starts listening for
    /// new messages from the signalling server and from other nodes.
    /// The actual logic is handled in Logic.
    pub async fn new(
        storage: Box<dyn DataStorageBase>,
        node_config: NodeConfig,
        _client: &str,
        ws: Broker<WSClientMessage>,
        web_rtc: WebRTCSpawner,
    ) -> Result<Node, NodeError> {
        let broker_net = Network::start(node_config.clone(), ws, web_rtc).await?;
        info!(
            "Starting node: {} = {}",
            node_config.our_node.info,
            node_config.our_node.get_id()
        );
        let mut node_data = NodeData::start(storage, node_config, broker_net.clone()).await?;
        node_data.add_timer(BrokerTimer::start()).await;

        Ok(Node {
            node_data,
            broker_net,
        })
    }

    /// Return a copy of the current node information
    pub fn info(&self) -> NodeInfo {
        self.node_data.node_config.our_node.clone()
    }

    /// Requests a list of all connected nodes
    pub async fn list(&mut self) -> Result<(), NodeError> {
        self.broker_net
            .emit_msg(NetCall::SendWSUpdateListRequest.into())
            .await?;
        Ok(())
    }

    /// Start processing of network and logic messages, in case they haven't been
    /// called automatically.
    /// Also updates all storage fields in the node_data field.
    pub async fn process(&mut self) -> Result<(), NodeError> {
        self.node_data.process().await?;
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
        self.nodes_info(self.node_data.random.storage.connected.get_nodes().0)
    }

    /// Returns all currently online nodes in the whole system. Every node will only connect
    /// to a subset of these nodes, which can be get with `nodes_connected`.
    pub fn nodes_online(&self) -> Result<Vec<NodeInfo>, NodeError> {
        self.nodes_info(self.node_data.random.storage.known.0.clone())
    }

    /// Returns a list of known nodes from the local storage
    pub fn nodes_info_all(&self) -> Result<HashMap<NodeID, NodeInfo>, NodeError> {
        let events = self.node_data.gossip.events(Category::NodeInfo);

        let mut nodeinfos = HashMap::new();
        for ni in events {
            // For some reason I cannot get to work the from_str in a .iter().map()
            match serde_yaml::from_str::<NodeInfo>(&ni.msg) {
                Ok(info) => {
                    nodeinfos.insert(info.get_id(), info);
                }
                Err(e) => log::error!("Parse-error {e:?} for {}", ni.msg),
            }
        }
        Ok(nodeinfos)
    }

    /// Returns the data about pinging the nodes
    pub fn nodes_ping(&self) -> PingStorage {
        self.node_data.ping.storage.clone()
    }

    /// Returns the stat data of the connected nodes
    pub fn nodes_stat(&self) -> HashMap<U256, NetworkConnectionState> {
        self.node_data.stat.states.clone()
    }

    /// Adds a new chat message that will be broadcasted to the system.
    pub async fn add_chat_message(&mut self, msg: String) -> Result<(), NodeError> {
        let event = events::Event {
            category: events::Category::TextMessage,
            src: self.node_data.node_config.our_node.get_id(),
            created: now(),
            msg,
        };
        self.node_data.gossip.add_event(event).await?;
        Ok(())
    }

    /// Returns all chat messages from the local storage.
    pub fn get_chat_messages(&self) -> Vec<events::Event> {
        self.node_data.gossip.chat_events()
    }

    /// Static method

    /// Updates the config of the node
    pub fn set_config(mut storage: Box<dyn DataStorage>, config: &str) -> Result<(), NodeError> {
        storage.set(CONFIG_NAME, config)?;
        Ok(())
    }
}
