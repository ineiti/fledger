use std::collections::HashSet;

use crate::{loopix::storage::{ClientStorage, LoopixStorage, ProviderStorage}, nodeconfig::NodeInfo};
use flarch::nodeids::NodeID;
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey, StaticSecret};

//////////////////////////////////////// ROLE ENUM ////////////////////////////////////////
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum LoopixRole {
    Mixnode,
    Provider,
    Client,
}

//////////////////////////////////////// Loopix Config ////////////////////////////////////////
#[derive(Debug, Clone, PartialEq)]
pub struct LoopixConfig {
    pub role: LoopixRole,
    pub core_config: CoreConfig,
    pub storage_config: LoopixStorage,
}

impl LoopixConfig {
    pub fn new_default(role: LoopixRole, path_length: usize, storage: LoopixStorage) -> Self {
        let core_config = match role {
            LoopixRole::Mixnode => CoreConfig::default_mixnode(path_length),
            LoopixRole::Provider => CoreConfig::default_provider(path_length),
            LoopixRole::Client => CoreConfig::default_client(path_length),
        };
        LoopixConfig {
            role,
            core_config,
            storage_config: storage,
        }
    }

    pub fn new(role: LoopixRole, storage: LoopixStorage, core_config: CoreConfig) -> Self {
        let network_storage = storage.network_storage.blocking_read().clone();
        let mixes = network_storage.get_mixes();
        if core_config.path_length() != mixes.len() {
            panic!("Path length in core config does not match path length in storage");
        }
        LoopixConfig {
            role,
            core_config,
            storage_config: storage,
        }
    }

    pub fn default_with_path_length(
        role: LoopixRole,
        our_node_id: NodeID,
        path_length: usize,
        private_key: StaticSecret,
        public_key: PublicKey,
        all_nodes: Vec<NodeInfo>,
    ) -> Self {
        let client_storage = match role {
            LoopixRole::Client => {
                Some(ClientStorage::default_with_path_length(
                    our_node_id,
                    all_nodes.clone(),
                    path_length,
                ))
            }
            _ => None,
        };

        let provider_storage = match role {
            LoopixRole::Provider => {
                let all_clients = all_nodes
                    .clone()
                    .into_iter()
                    .take(path_length)
                    .collect::<Vec<NodeInfo>>();

                Some(ProviderStorage::default_with_path_length(
                    HashSet::from_iter(all_clients.iter().map(|node| node.get_id())),
                ))
            }
            _ => None,
        };

        if (client_storage.is_none() 
            && provider_storage.is_none() 
            && !all_nodes
                .clone()
                .into_iter()
                .skip(path_length * 2)
                .any(|node| node.get_id() == our_node_id))
            && (role == LoopixRole::Mixnode)
        {
            log::error!("Node ID {} not found in {:?}", our_node_id, all_nodes);
            panic!("Our node id must be between the path length and 2 times the path length");
        }
        
        let storage = LoopixStorage::default_with_path_length(
            our_node_id,
            path_length,
            private_key,
            public_key,
            client_storage,
            provider_storage,
            all_nodes,
        );

        LoopixConfig::new_default(role, path_length, storage)
    }
}

//////////////////////////////////////// Core Config ////////////////////////////////////////
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct CoreConfig {
    lambda_loop: f64,
    lambda_drop: f64,
    lambda_payload: f64,
    path_length: usize,
    mean_delay: u64,
    lambda_loop_mix: f64,
    time_pull: f64,
    max_retrieve: usize,
    pad_length: usize
}

impl CoreConfig {
    pub fn default_mixnode(path_length: usize) -> Self {
        CoreConfig {
            // lambda_loop: 0.0, // loop rate (10 times per minute)
            // lambda_drop: 0.0, // drop rate (10 times per minute)
            lambda_loop: 6.0, // loop rate (10 times per minute)
            lambda_drop: 6.0, // drop rate (10 times per minute)
            lambda_payload: 0.0, // payload rate (0 times per minute)
            path_length,
            mean_delay: 1, // mean delay (1ms)
            lambda_loop_mix: 60.0, // loop mix rate (10 times per minute)
            time_pull: 0.0, // time pull (3 second)
            max_retrieve: 0, // messages sent to client per pull request
            pad_length: 150, // dummy, drop, loop messages are padded
        }
    }

    pub fn default_provider(path_length: usize) -> Self {
        CoreConfig {
            lambda_loop: 6.0,
            lambda_drop: 6.0,
            // lambda_loop: 0.0,
            // lambda_drop: 0.0,
            lambda_payload: 0.0,
            path_length,
            mean_delay: 1,
            lambda_loop_mix: 60.0,
            time_pull: 0.0,
            max_retrieve: 5,
            pad_length: 150,
        }
    }

    pub fn default_client(path_length: usize) -> Self {
        CoreConfig {
            lambda_loop: 6.0,
            lambda_drop: 6.0,
            // lambda_loop: 0.0,
            // lambda_drop: 0.0,
            lambda_payload: 60.0,
            path_length,
            mean_delay: 1,
            lambda_loop_mix: 0.0,
            time_pull: 1.0,
            max_retrieve: 0,
            pad_length: 150,
        }
    }
}

impl Default for CoreConfig {
    fn default() -> Self {
        CoreConfig {
            lambda_loop: 60.0,
            lambda_drop: 60.0,
            lambda_payload: 60.0,
            path_length: 3,
            mean_delay: 1,
            lambda_loop_mix: 60.0,
            time_pull: 3.0,
            max_retrieve: 10,
            pad_length: 150,
        }
    }
}

impl CoreConfig {
    pub fn lambda_loop(&self) -> f64 {
        self.lambda_loop
    }

    pub fn lambda_drop(&self) -> f64 {
        self.lambda_drop
    }

    pub fn lambda_payload(&self) -> f64 {
        self.lambda_payload
    }

    pub fn path_length(&self) -> usize {
        self.path_length as usize
    }

    pub fn mean_delay(&self) -> u64 {
        self.mean_delay
    }

    pub fn lambda_loop_mix(&self) -> f64 {
        self.lambda_loop_mix
    }

    pub fn time_pull(&self) -> f64 {
        self.time_pull
    }

    pub fn max_retrieve(&self) -> usize {
        self.max_retrieve
    }

    pub fn pad_length(&self) -> usize {
        self.pad_length
    }
}
