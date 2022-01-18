use crate::node::BrokerError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use thiserror::Error;

use types::{data_storage::StorageError, nodeids::U256};

const MESSAGE_MAXIMUM: usize = 20;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct TextMessagesStorage {
    pub storage: HashMap<U256, TextMessage>,
}

impl TextMessagesStorage {
    pub fn new() -> Self {
        Self {
            storage: HashMap::new(),
        }
    }

    pub fn load(&mut self, data: &str) -> Result<(), TMError> {
        if data.len() > 0 {
            let msg_vec: Vec<TextMessage> = serde_json::from_str(data)?;
            self.storage.clear();
            for msg in msg_vec {
                self.storage.insert(msg.id(), msg);
            }
        } else {
            self.storage = HashMap::new();
        }
        Ok(())
    }

    pub fn save(&self) -> Result<String, TMError> {
        let msg_vec: Vec<TextMessage> = self.storage.iter().map(|(_k, v)| v).cloned().collect();
        Ok(serde_json::to_string(&msg_vec)?.into())
    }

    // Limit the number of messages to MESSAGE_MAXIMUM,
    // returns the ids of deleted messages.
    pub fn limit_messages(&mut self) -> Vec<U256> {
        if self.storage.len() > MESSAGE_MAXIMUM {
            let mut msgs = self
                .storage
                .iter()
                .map(|(_k, v)| v.clone())
                .collect::<Vec<TextMessage>>();
            msgs.sort_by(|a, b| b.created.partial_cmp(&a.created).unwrap());
            msgs.drain(0..MESSAGE_MAXIMUM);
            for msg in msgs.iter() {
                self.storage.remove(&msg.id());
            }
            msgs.iter().map(|msg| msg.id()).collect()
        } else {
            vec![]
        }
    }
}

#[derive(Error, Debug)]
pub enum TMError {
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Broker(#[from] BrokerError),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextMessage {
    pub node_info: String,
    pub src: U256,
    pub created: f64,
    pub liked: u64,
    pub msg: String,
}

impl TextMessage {
    pub fn id(&self) -> U256 {
        let mut id = Sha256::new();
        id.update(&self.src);
        id.update(&self.created.to_le_bytes());
        id.update(&self.liked.to_le_bytes());
        id.update(&self.msg);
        id.finalize().into()
    }
}
