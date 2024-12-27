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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

    pub async fn new(role: LoopixRole, storage: LoopixStorage, core_config: CoreConfig) -> Self {
        let network_storage = storage.network_storage.read().await.clone();
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
                Some(ProviderStorage::default_with_path_length())
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

    pub fn default_with_core_config_and_path_length(
        role: LoopixRole,
        our_node_id: NodeID,
        path_length: usize,
        private_key: StaticSecret,
        public_key: PublicKey,
        all_nodes: Vec<NodeInfo>,
        core_config: CoreConfig,
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
                Some(ProviderStorage::default_with_path_length())
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

        LoopixConfig {
            role,
            core_config,
            storage_config: storage,
        }
    }

    pub fn default_with_path_length_and_n_clients(
        role: LoopixRole,
        our_node_id: NodeID,
        path_length: usize,
        n_clients: usize,
        private_key: StaticSecret,
        public_key: PublicKey,
        all_nodes: Vec<NodeInfo>,
        core_config: CoreConfig,
    ) -> Self {
        let client_storage = match role {
            LoopixRole::Client => {
                Some(ClientStorage::default_with_path_length_and_n_clients(
                    our_node_id,
                    all_nodes.clone(),
                    path_length,
                    n_clients,
                ))
            }
            _ => None,
        };

        let provider_storage = match role {
            LoopixRole::Provider => {
                Some(ProviderStorage::default_with_path_length())
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
        
        let storage = LoopixStorage::default_with_path_length_and_n_clients(
            our_node_id,
            path_length,
            n_clients,
            private_key,
            public_key,
            client_storage,
            provider_storage,
            all_nodes,
        );

        LoopixConfig {
            role,
            core_config,
            storage_config: storage,
        }
    }
}

//////////////////////////////////////// Core Config ////////////////////////////////////////
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct CoreConfig {
    lambda_loop: f64, // Loop traffic rate (user) (per second)
    lambda_drop: f64, // Drop cover traffic rate (user) (per second)
    lambda_payload: f64, // Payload traffic rate (user) (per second)
    path_length: usize, // Path length (user)
    mean_delay: u64, // The mean delay at mix Mi (milliseconds)
    lambda_loop_mix: f64, // Loop traffic rate (mix) (per second)
    time_pull: f64, // Time pull (user) (seconds)
    max_retrieve: usize, // Max retrieve (provider)
    pad_length: usize // TODO implement
}

impl CoreConfig {
    pub fn default_mixnode(path_length: usize) -> Self {
        CoreConfig {
            lambda_loop: 10.0, // loop rate (10 messages per second)
            lambda_drop: 10.0, // drop rate (10 messages per second)
            lambda_payload: 0.0, // payload rate (0 messages per second)
            path_length,
            mean_delay: 2, // mean delay (ms)
            lambda_loop_mix: 10.0, // loop mix rate (10 messages per second)
            time_pull: 0.0, // time pull (3 second)
            max_retrieve: 0, // messages sent to client per pull request
            pad_length: 150, // dummy, drop, loop messages are padded
        }
    }

    pub fn default_provider(path_length: usize) -> Self {
        CoreConfig {
            lambda_loop: 10.0,
            lambda_drop: 10.0,
            lambda_payload: 0.0,
            path_length,
            mean_delay: 2,
            lambda_loop_mix: 10.0,
            time_pull: 0.0,
            max_retrieve: 5,
            pad_length: 150,
        }
    }

    pub fn default_client(path_length: usize) -> Self {
        CoreConfig {
            lambda_loop: 10.0,
            lambda_drop: 10.0,
            lambda_payload: 120.0,
            path_length,
            mean_delay: 2,
            lambda_loop_mix: 0.0,
            time_pull: 3.0,
            max_retrieve: 0,
            pad_length: 150,
        }
    }
}

impl Default for CoreConfig {
    fn default() -> Self {
        CoreConfig {
            lambda_loop: 10.0,
            lambda_drop: 10.0,
            lambda_payload: 120.0,
            path_length: 3,
            mean_delay: 2,
            lambda_loop_mix: 10.0,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn test_write_core_config_to_file() {
        let config = CoreConfig::default();
        let json = serde_yaml::to_string(&config).unwrap();
        
        let mut path = PathBuf::from(".");
        path.push("test_core_config.yaml");
        
        fs::write(&path, json).unwrap();
        
        let read_json = fs::read_to_string(&path).unwrap();
        let read_config: CoreConfig = serde_yaml::from_str(&read_json).unwrap();
        
        assert_eq!(config, read_config);
        
        fs::remove_file(path).unwrap();
    }
}

