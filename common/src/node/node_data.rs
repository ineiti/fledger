use std::sync::{Arc, Mutex};

use crate::{
    broker::Broker,
    node::{config::NodeConfig, stats::NDStats, network::NetworkState},
};

use types::data_storage::DataStorageBase;

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
    /// Handles a random number of connections
    pub random_connections: raw::random_connections::Module,
    /// Gossip-messages sent and received
    pub gossip_chat: raw::gossip_events::Module,
}

impl NodeData {
    pub fn new(node_config: NodeConfig, storage: Box<dyn DataStorageBase>) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            storage,
            broker: Broker::new(),
            network_state: NetworkState::new(),
            stats: NDStats::new(),
            random_connections: raw::random_connections::Module::new(
                raw::random_connections::Config::default(),
            ),
            gossip_chat: raw::gossip_events::Module::new(
                raw::gossip_events::Config::new(node_config.our_node.get_id()),
            ),
            node_config,
        }))
    }
}
