use log::{error, info};
use std::{
    collections::HashMap,
    convert::TryFrom,
    sync::{Arc, Mutex},
};
use thiserror::Error;

use raw::gossip_events::events;
use types::{
    data_storage::{DataStorage, DataStorageBase, StorageError},
    nodeids::U256,
    utils::now,
};

use crate::{
    broker::{Broker, BrokerError, BrokerMessage, BrokerModules},
    node::modules::{
        gossip_events::{self, GossipChat, GossipMessage},
        random_connections::RandomConnections,
    },
    node::{
        config::{ConfigError, NodeConfig, NodeInfo},
        network::{Network, NetworkError},
        stats::StatNode,
    },
    node::{network::BrokerNetwork, node_data::NodeData, stats::Stats, timer::Timer},
    signal::{web_rtc::WebRTCSpawner, websocket::WebSocketConnection},
};

pub mod config;
pub mod modules;
pub mod network;
pub mod node_data;
pub mod stats;
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
    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),
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
        // New config place
        let mut storage_node = storage.get("fledger");

        // First try the old config
        let mut old_storage_node = storage.get("");
        let mut config_str = old_storage_node.get(CONFIG_NAME)?;
        if !config_str.is_empty() {
            info!("Migrating config");
            storage_node.set(CONFIG_NAME, &config_str)?;
            old_storage_node.remove(CONFIG_NAME)?;
        }

        config_str = match storage_node.get(CONFIG_NAME) {
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

        let node_data = NodeData::new(config.clone(), storage);
        let broker = {
            Stats::start(Arc::clone(&node_data));
            RandomConnections::start(Arc::clone(&node_data));
            GossipChat::start(Arc::clone(&node_data));
            node_data.lock().unwrap().broker.clone()
        };
        Network::start(broker.clone(), config, ws, web_rtc);
        Timer::start(broker.clone());

        Ok(Node { node_data, broker })
    }

    /// Return a copy of the current node information
    pub fn info(&self) -> Result<NodeInfo, NodeError> {
        let state = self.node_data.lock().map_err(|_| NodeError::Lock)?;
        Ok(state.node_config.clone().our_node)
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
    pub fn get_list_active(&self) -> Result<Vec<NodeInfo>, NodeError> {
        let nd = self.node_data.lock().map_err(|_| NodeError::Lock)?;
        Ok(nd
            .stats
            .nodes
            .values()
            .filter_map(|sd| sd.node_info.as_ref())
            .cloned()
            .collect())
    }

    /// Returns a list of known nodes from the local storage
    pub fn get_list_nodes(&self) -> Result<Vec<NodeInfo>, NodeError> {
        let nd = self.node_data.lock().map_err(|_| NodeError::Lock)?;
        let mut nodeinfos = vec![];
        for ni in nd.gossip_events.get_events(events::Category::NodeInfo) {
            // For some reason I cannot get to work the from_str in a .iter().map()
            nodeinfos.push(serde_yaml::from_str(&ni.msg)?);
        }
        Ok(nodeinfos)
    }

    /// Returns a copy of the logic stats
    pub fn stats(&self) -> Result<HashMap<U256, StatNode>, NodeError> {
        let nd = self.node_data.try_lock().map_err(|_| NodeError::Lock)?;
        Ok(nd.stats.nodes.clone())
    }

    pub fn add_chat_message(&self, msg: String) -> Result<(), NodeError> {
        let msg_txt = events::Event {
            category: events::Category::TextMessage,
            src: self.info()?.get_id(),
            created: now(),
            msg,
        };
        self.broker
            .enqueue_bm(BrokerMessage::Modules(BrokerModules::Gossip(
                GossipMessage::MessageIn(gossip_events::MessageIn::AddEvent(msg_txt)),
            )));
        Ok(())
    }

    pub fn get_chat_messages(&self) -> Result<Vec<events::Event>, NodeError> {
        if let Ok(nd) = self.node_data.try_lock() {
            return Ok(nd
                .gossip_events
                .get_chat_events(events::Category::TextMessage));
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
