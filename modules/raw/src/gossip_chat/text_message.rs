use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use common::types::U256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextMessage {
    pub src: U256,
    pub created: f64,
    pub msg: String,
}

impl TextMessage {
    pub fn id(&self) -> U256 {
        let mut id = Sha256::new();
        id.update(&self.src);
        id.update(&self.created.to_le_bytes());
        id.update(&self.msg);
        id.finalize().into()
    }
}
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct TextMessagesStorage {
    storage: HashMap<U256, TextMessage>,
    maximum: usize,
}

impl TextMessagesStorage {
    pub fn new(maximum: usize) -> Self {
        Self {
            storage: HashMap::new(),
            maximum,
        }
    }

    pub fn load(&mut self, data: &str) -> Result<(), serde_json::Error> {
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

    pub fn save(&self) -> Result<String, serde_json::Error> {
        let msg_vec: Vec<TextMessage> = self.storage.iter().map(|(_k, v)| v).cloned().collect();
        Ok(serde_json::to_string(&msg_vec)?.into())
    }

    pub fn add_message(&mut self, msg: TextMessage) -> bool {
        if self.storage.contains_key(&msg.id())
            || (self.storage.len() >= self.maximum && self.created_before(msg.created))
        {
            return false;
        }
        self.storage.insert(msg.id(), msg);
        self.limit_messages();
        true
    }

    /// Stores all new messages and returns the new messages.
    pub fn add_messages(&mut self, msgs: Vec<TextMessage>) -> Vec<TextMessage>{
        msgs.iter().filter(|&tm| self.add_message(tm.clone())).cloned().collect()
    }

    pub fn get_messages(&self) -> Vec<TextMessage> {
        self.storage.iter().map(|(_k, v)| v.clone()).collect()
    }

    pub fn get_message_ids(&self) -> Vec<U256> {
        self.storage.iter().map(|(k, _v)| k.clone()).collect()
    }

    pub fn get_message(&self, id: &U256) -> Option<TextMessage> {
        self.storage.get(id).and_then(|tm| Some(tm.clone()))
    }

    pub fn contains(&self, id: &U256) -> bool {
        self.storage.contains_key(id)
    }

    fn limit_messages(&mut self) {
        if self.storage.len() > self.maximum {
            let mut msgs = self
                .storage
                .iter()
                .map(|(_k, v)| v.clone())
                .collect::<Vec<TextMessage>>();
            msgs.sort_by(|a, b| b.created.partial_cmp(&a.created).unwrap());
            msgs.drain(0..self.maximum);
            for msg in msgs.iter() {
                self.storage.remove(&msg.id());
            }
        }
    }

    fn created_before(&self, created: f64) -> bool {
        if let Some(tm) = self
            .storage
            .values()
            .min_by(|x, y| x.created.partial_cmp(&y.created).unwrap())
        {
            return tm.created >= created;
        }
        return false;
    }
}
