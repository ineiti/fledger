use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use types::nodeids::U256;

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

    pub fn load(&mut self, data: &str) -> Result<(), serde_json::Error> {
        if !data.is_empty() {
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
}

impl Default for TextMessagesStorage {
    fn default() -> Self {
        Self::new()
    }
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
