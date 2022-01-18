use crate::broker::ModulesMessage;
use crate::node::modules::gossip_chat::{self, GossipChat, GossipMessage};
use crate::node::modules::random_connections::RandomConnections;
use crate::types::now;
use crate::{
    broker::BrokerMessage,
    node::{network::BrokerNetwork, timer::Timer},
};
use std::{
    collections::HashMap,
    convert::TryFrom,
    sync::{Arc, Mutex},
};
use thiserror::Error;

use log::{error, info};

use self::{
    config::{ConfigError, NodeConfig, NodeInfo},
    logic::stats::StatNode,
    network::{Network, NetworkError},
};
use crate::{
    broker::{Broker, BrokerError},
    node::{logic::stats::Stats, node_data::NodeData},
    signal::{web_rtc::WebRTCSpawner, websocket::WebSocketConnection},
};
use raw::gossip_chat::text_message::TextMessage;
use types::{
    data_storage::{DataStorage, DataStorageBase, StorageError},
    nodeids::U256,
};

pub mod config;
pub mod logic;
pub mod modules;
pub mod network;
pub mod node_data;
pub mod timer;
pub mod version;

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
}

/// The node structure holds it all together. It is the main structure of the project.
pub struct Node {
    node_data: Arc<Mutex<NodeData>>,
    broker: Broker,
}

pub const CONFIG_NAME: &str = "nodeConfig";

impl Node {
    /// Create new node by loading the config from the storage.
    /// This also initializes the network and starts listening for
    /// new messages from the signalling server and from other nodes.
    /// The actual logic is handled in Logic.
    pub fn new(
        storage: Box<dyn DataStorageBase>,
        client: &str,
        ws: Box<dyn WebSocketConnection>,
        web_rtc: WebRTCSpawner,
    ) -> Result<Node, NodeError> {
        let mut storage_node = storage.get("fledger");
        let config_str = match storage_node.get(CONFIG_NAME) {
            Ok(s) => s,
            Err(_) => {
                info!("Couldn't load configuration - start with empty");
                "".to_string()
            }
        };
        let mut config = NodeConfig::try_from(config_str)?;
        config.our_node.client = client.to_string();
        storage_node.set(CONFIG_NAME, &config.to_string()?)?;
        info!(
            "Starting node: {} = {}",
            config.our_node.info,
            config.our_node.get_id()
        );

        let node_data = NodeData::new(config, storage);
        let broker = {
            Network::new(Arc::clone(&node_data), ws, web_rtc);
            Stats::new(Arc::clone(&node_data));
            RandomConnections::new(Arc::clone(&node_data));
            GossipChat::new(Arc::clone(&node_data));
            node_data.lock().unwrap().broker.clone()
        };
        Timer::new(broker.clone());

        Ok(Node { node_data, broker })
    }

    /// Return a copy of the current node information
    pub fn info(&self) -> Result<NodeInfo, NodeError> {
        let state = self.node_data.lock().map_err(|_| NodeError::Lock)?;
        Ok(state.node_config.clone().our_node)
    }

    /// TODO: this is only for development
    pub fn clear(&mut self) -> Result<(), NodeError> {
        Ok(self
            .broker
            .emit_bm(BrokerMessage::Network(BrokerNetwork::ClearNodes))
            .map(|_| ())?)
    }

    /// Requests a list of all connected nodes
    pub fn list(&mut self) -> Result<(), NodeError> {
        Ok(self
            .broker
            .emit_bm(BrokerMessage::Network(BrokerNetwork::UpdateListRequest))
            .map(|_| ())?)
    }

    /// Start processing of network and logic messages, in case they haven't been
    /// called automatically.
    pub async fn process(&mut self) -> Result<usize, NodeError> {
        Ok(self.broker.process()?)
    }

    /// Gets the current list of available nodes
    pub fn get_list(&mut self) -> Result<Vec<NodeInfo>, NodeError> {
        let nd = self.node_data.lock().map_err(|_| NodeError::Lock)?;
        Ok(nd.network_state.list.clone())
    }

    /// Returns a copy of the logic stats
    pub fn stats(&self) -> Result<HashMap<U256, StatNode>, NodeError> {
        let nd = self.node_data.try_lock().map_err(|_| NodeError::Lock)?;
        Ok(nd.stats.nodes.clone())
    }

    pub fn add_message(&self, msg: String) -> Result<(), NodeError> {
        self.broker
            .enqueue_bm(BrokerMessage::Modules(ModulesMessage::Gossip(
                GossipMessage::MessageIn(gossip_chat::MessageIn::AddMessage(now(), msg)),
            )));
        Ok(())
    }

    pub fn get_messages(&self) -> Result<Vec<TextMessage>, NodeError> {
        if let Ok(nd) = self.node_data.try_lock() {
            return Ok(nd.gossip_chat.get_messages());
        }
        Err(NodeError::Lock)
    }

    /// Static method

    /// Updates the config of the node
    pub fn set_config(mut storage: Box<dyn DataStorage>, config: &str) -> Result<(), NodeError> {
        storage.set(CONFIG_NAME, config)?;
        Ok(())
    }
}
