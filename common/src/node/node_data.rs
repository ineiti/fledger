use crate::node::logic::text_messages::TextMessagesStorage;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::{
    broker::Broker,
    node::{config::NodeConfig, logic::stats::NDStats, network::NetworkState},
};

use types::data_storage::{DataStorage, DataStorageBase, StorageError};

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
    pub storage: Box<dyn DataStorageBase>,
    /// Broker that can be cloned
    pub broker: Broker,

    // Subsystem data
    /// State of the network
    pub network_state: NetworkState,
    /// Statistics of the connection
    pub stats: NDStats,
    /// This is the old way of storing messages - only used to copy messages to the gossip_messages field.
    pub messages: TextMessagesStorage,
    /// All modules used in fledger
    pub modules: Modules,
}

pub struct Modules {
    /// Handles a random number of connections
    pub random_connections: raw::random_connections::Module,
    /// Gossip-messages sent and received
    pub gossip_messages: raw::gossip_chat::text_message::TextMessagesStorage,
}

pub struct TempDSB {}

impl TempDSB {
    pub fn new() -> Box<Self> {
        Box::new(Self {})
    }
}

impl DataStorageBase for TempDSB {
    fn get(&self, _: &str) -> Box<dyn DataStorage> {
        TempDS::new()
    }
    fn clone(&self) -> Box<dyn DataStorageBase> {
        TempDSB::new()
    }
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
    fn get(&self, key: &str) -> Result<String, StorageError> {
        Ok(self.kvs.get(key).unwrap_or(&"".to_string()).clone())
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), StorageError> {
        self.kvs.insert(key.to_string(), value.to_string());
        Ok(())
    }
}

impl NodeData {
    pub fn new(node_config: NodeConfig, storage: Box<dyn DataStorageBase>) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            node_config,
            storage,
            broker: Broker::new(),
            network_state: NetworkState::new(),
            stats: NDStats::new(),
            messages: TextMessagesStorage::new(),
            modules: Modules {
                random_connections: raw::random_connections::Module::new(None),
                gossip_messages: raw::gossip_chat::text_message::TextMessagesStorage::new(100),
            },
        }))
    }

    pub fn new_test() -> Arc<Mutex<Self>> {
        Self::new(NodeConfig::new(), TempDSB::new())
    }
}
