use itertools::Itertools;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use types::nodeids::U256;

/// This holds a number of Messages from of different categories. Every category can
/// have its own configuration with regard of whether its messages are unique to a
/// node (e.g., NodeInfo), or can be many per node (e.g., TextMessage).
#[derive(Debug, Serialize, Deserialize)]
pub struct MessageStorage {
    storage: HashMap<Category, Messages>,
}

impl MessageStorage {
    /// Initializes a MessageStorage with two categories.
    pub fn new() -> Self {
        let mut storage = HashMap::new();
        storage.insert(
            Category::TextMessage,
            Messages {
                config: CategoryConfig {
                    unique: false,
                    max_messages: 20,
                },
                messages: HashMap::new(),
            },
        );
        storage.insert(
            Category::NodeInfo,
            Messages {
                config: CategoryConfig {
                    unique: true,
                    max_messages: 50,
                },
                messages: HashMap::new(),
            },
        );
        Self { storage }
    }

    pub fn add_message(&mut self, msg: Message) -> bool {
        let mut modified = false;
        self.storage.entry(msg.category).and_modify(|msgs| {
            modified = msgs.insert(msg);
        });
        modified
    }

    pub fn get_message(&self, id: &U256) -> Option<Message> {
        for msgs in self.storage.values() {
            if let Some(msg) = msgs.get_message(id) {
                return Some(msg);
            }
        }
        None
    }

    pub fn get_messages(&self, cat: Category) -> Vec<Message> {
        self.storage
            .get(&cat)
            .unwrap()
            .messages
            .values()
            .cloned()
            .collect()
    }

    pub fn get_messages_by_ids(&self, ids: Vec<U256>) -> Vec<Message> {
        self.storage
            .values()
            .flat_map(|msgs| msgs.get_messages_by_ids(ids.clone()))
            .collect()
    }

    pub fn get_message_ids(&self) -> Vec<U256> {
        self.storage
            .values()
            .flat_map(|msgs| msgs.messages.values().map(|msg| msg.get_id()))
            .collect()
    }

    pub fn get(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn set(&mut self, data: &str) -> Result<(), serde_json::Error> {
        self.storage = serde_json::from_str::<MessageStorage>(data)?.storage;
        Ok(())
    }
}

impl Default for MessageStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Messages {
    config: CategoryConfig,
    messages: HashMap<U256, Message>,
}

impl Messages {
    // Makes sure that the 'unique' configuration is respected.
    pub fn insert(&mut self, msg: Message) -> bool {
        if self.config.unique {
            self.try_insert(msg.src, msg)
        } else {
            self.try_insert(msg.get_id(), msg)
        }
    }

    fn try_insert(&mut self, id: U256, msg: Message) -> bool {
        if let Some(msg_stored) = self.messages.get(&id) {
            if msg_stored.created >= msg.created {
                return false;
            }
        }
        self.messages.insert(id, msg);
        self.limit();
        true
    }

    /// Returns all messages that are part of the ids.
    pub fn get_messages_by_ids(&self, ids: Vec<U256>) -> Vec<Message> {
        if self.config.unique {
            log::trace!("Looking for unique messages {:?}", ids);
            return self
                .messages
                .values()
                .filter(|&msg| ids.contains(&msg.get_id()))
                .cloned()
                .collect();
        }
        log::trace!("Looking for normal messages {:?} in {:?}", ids, self.messages);
        ids.iter()
            .filter_map(|id| self.messages.get(id))
            .cloned()
            .collect()
    }

    pub fn get_message(&self, id: &U256) -> Option<Message> {
        self.get_messages_by_ids(vec![*id]).into_iter().next()
    }

    // Ensures that there are no more than max_messages stored.
    // Deletes the oldest messages if there are more.
    fn limit(&mut self) {
        if self.messages.len() <= self.config.max_messages {
            return;
        }
        let ids: Vec<U256> = self
            .messages
            .iter()
            .map(|(k, v)| (k, v.created))
            .sorted_by(|a, b| b.1.partial_cmp(&a.1).unwrap())
            .skip(self.config.max_messages)
            .map(|(k, _)| k)
            .cloned()
            .collect();
        for id in ids {
            self.messages.remove(&id);
        }
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Category {
    TextMessage,
    NodeInfo,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CategoryConfig {
    // Only one message per node
    unique: bool,
    // How many messages to hold
    max_messages: usize,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Message {
    pub category: Category,
    pub src: U256,
    pub created: f64,
    pub msg: String,
}

impl Message {
    pub fn get_id(&self) -> U256 {
        let mut id = Sha256::new();
        id.update(format!("{:?}", self.category));
        id.update(self.src);
        id.update(self.created.to_le_bytes());
        id.update(&self.msg);
        id.finalize().into()
    }
}
