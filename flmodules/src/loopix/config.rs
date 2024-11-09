use crate::loopix::storage::{ClientStorage, LoopixStorage, ProviderStorage};
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
        our_node_id: u32,
        path_length: usize,
        private_key: StaticSecret,
        public_key: PublicKey,
    ) -> Self {
        let client_storage = match role {
            LoopixRole::Client => Some(ClientStorage::default_with_path_length(
                our_node_id,
                path_length,
            )),
            _ => None,
        };

        let provider_storage = match role {
            LoopixRole::Provider => Some(ProviderStorage::default_with_path_length(
                our_node_id,
                path_length,
            )),
            _ => None,
        };

        if (client_storage.is_none() && provider_storage.is_none())
            && (role == LoopixRole::Mixnode)
            && our_node_id < (path_length * 2) as u32
        {
            panic!("Our node id must be between the path length and 2 times the path length");
        }

        let storage = LoopixStorage::default_with_path_length(
            our_node_id,
            path_length,
            private_key,
            public_key,
            client_storage,
            provider_storage,
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
    mean_delay: f64,
    lambda_loop_mix: f64,
    time_pull: f64,
    max_retrieve: usize,
}

impl CoreConfig {
    pub fn default_mixnode(path_length: usize) -> Self {
        // TODO: change these values
        CoreConfig {
            lambda_loop: 10.0,
            lambda_drop: 10.0,
            lambda_payload: 10.0,
            path_length,
            mean_delay: 0.001,
            lambda_loop_mix: 10.0,
            time_pull: 10.0,
            max_retrieve: 10,
        }
    }

    pub fn default_provider(path_length: usize) -> Self {
        CoreConfig {
            lambda_loop: 15.0,
            lambda_drop: 15.0,
            lambda_payload: 15.0,
            path_length,
            mean_delay: 0.002,
            lambda_loop_mix: 15.0,
            time_pull: 15.0,
            max_retrieve: 10,
        }
    }

    pub fn default_client(path_length: usize) -> Self {
        CoreConfig {
            lambda_loop: 20.0,
            lambda_drop: 20.0,
            lambda_payload: 20.0,
            path_length,
            mean_delay: 0.003,
            lambda_loop_mix: 20.0,
            time_pull: 20.0,
            max_retrieve: 10,
        }
    }
}

impl Default for CoreConfig {
    fn default() -> Self {
        CoreConfig {
            lambda_loop: 10.0,
            lambda_drop: 10.0,
            lambda_payload: 10.0,
            path_length: 3,
            mean_delay: 0.001,
            lambda_loop_mix: 10.0,
            time_pull: 10.0,
            max_retrieve: 10,
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

    pub fn mean_delay(&self) -> f64 {
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
}
