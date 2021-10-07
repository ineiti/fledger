use crate::{node::logic::text_messages::TextMessagesStorage, types::StorageError};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::{
    broker::Broker,
    node::{config::NodeConfig, logic::stats::NDStats, network::NetworkState},
    types::DataStorage,
};

/// The NodeState is the shared global state of every node.
/// It must only be stored as Arc<Mutex<NodeState>>.
/// Theoretically there should never be a collision for the mutex,
/// as wasm is single-threaded and should not be pre-emptively
/// interrupted.
/// But that assumption might be false with regard to websocket-
/// and webrtc-callbacks.
pub struct NodeData {
    /// The node configuration
    pub node_config: NodeConfig,
    /// Storage to be used
    pub storage: Box<dyn DataStorage>,
    /// Broker that can be cloned
    pub broker: Broker,

    // Subsystem data
    /// State of the network
    pub network_state: NetworkState,
    /// Statistics of the connection
    pub stats: NDStats,
    /// Messages sent and received
    pub messages: TextMessagesStorage,
}

pub struct TempDS {
    kvs: HashMap<String, String>,
}

impl TempDS {
    pub fn new() -> Box<Self> {
        Box::new(Self {
            kvs: HashMap::new(),
        })
    }
}

impl DataStorage for TempDS {
    fn load(&self, key: &str) -> Result<String, StorageError> {
        Ok(self.kvs.get(key).unwrap_or(&"".to_string()).clone())
    }

    fn save(&mut self, key: &str, value: &str) -> Result<(), StorageError> {
        self.kvs.insert(key.to_string(), value.to_string());
        Ok(())
    }
}

impl NodeData {
    pub fn new(node_config: NodeConfig, storage: Box<dyn DataStorage>) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            node_config,
            storage,
            broker: Broker::new(),
            network_state: NetworkState::new(),
            stats: NDStats::new(),
            messages: TextMessagesStorage::new(),
        }))
    }

    pub fn new_test() -> Arc<Mutex<Self>> {
        Self::new(NodeConfig::new(), TempDS::new())
    }
}
