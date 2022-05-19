use itertools::Itertools;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use flutils::nodeids::U256;

/// This holds a number of Events from of different categories. Every category can
/// have its own configuration with regard of whether its events are unique to a
/// node (e.g., NodeInfo), or can be many per node (e.g., TextMessage).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct EventsStorage {
    storage: HashMap<Category, Events>,
}

impl EventsStorage {
    /// Initializes an EventsStorage with two categories.
    pub fn new() -> Self {
        let mut storage = HashMap::new();
        storage.insert(
            Category::TextMessage,
            Events {
                config: CategoryConfig {
                    unique: false,
                    max_events: 20,
                },
                events: HashMap::new(),
            },
        );
        storage.insert(
            Category::NodeInfo,
            Events {
                config: CategoryConfig {
                    unique: true,
                    max_events: 50,
                },
                events: HashMap::new(),
            },
        );
        Self { storage }
    }

    pub fn add_event(&mut self, msg: Event) -> bool {
        let mut modified = false;
        self.storage.entry(msg.category).and_modify(|msgs| {
            modified = msgs.insert(msg);
        });
        modified
    }

    pub fn event(&self, id: &U256) -> Option<Event> {
        for msgs in self.storage.values() {
            if let Some(msg) = msgs.get_event(id) {
                return Some(msg);
            }
        }
        None
    }

    pub fn events(&self, cat: Category) -> Vec<Event> {
        self.storage
            .get(&cat)
            .unwrap()
            .events
            .values()
            .cloned()
            .collect()
    }

    pub fn get_events_by_ids(&self, ids: Vec<U256>) -> Vec<Event> {
        self.storage
            .values()
            .flat_map(|msgs| msgs.get_events_by_ids(ids.clone()))
            .collect()
    }

    pub fn event_ids(&self) -> Vec<U256> {
        self.storage
            .values()
            .flat_map(|msgs| msgs.events.values().map(|msg| msg.get_id()))
            .collect()
    }

    pub fn get(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string(self)
    }

    pub fn set(&mut self, data: &str) -> Result<(), serde_yaml::Error> {
        self.storage = serde_yaml::from_str::<EventsStorage>(data)?.storage;
        Ok(())
    }
}

impl Default for EventsStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Events {
    config: CategoryConfig,
    events: HashMap<U256, Event>,
}

impl Events {
    // Makes sure that the 'unique' configuration is respected.
    pub fn insert(&mut self, msg: Event) -> bool {
        if self.events.get(&msg.get_id()).is_some() {
            return false;
        }
        if self.config.unique {
            self.insert_unique(msg)
        } else {
            self.insert_simple(msg)
        }
    }

    // Check if a message from the same node already exists. If it does,
    // keep only the most recent of the stored and incoming message.
    fn insert_unique(&mut self, msg: Event) -> bool {
        if let Some((id, ev)) = self
            .events
            .iter()
            .find(|(_, v)| v.src == msg.src).map(|(id, ev)| (*id, ev.clone()))
        {
            if ev.created > msg.created {
                return false;
            }
            self.events.remove(&id);
        }
        self.insert_simple(msg)
    }

    // Insert the message in the events and make sure the limits are kept.
    fn insert_simple(&mut self, msg: Event) -> bool {
        self.events.insert(msg.get_id(), msg);
        self.limit();
        true
    }

    /// Returns all messages that are part of the ids.
    pub fn get_events_by_ids(&self, ids: Vec<U256>) -> Vec<Event> {
        if self.config.unique {
            return self
                .events
                .values()
                .filter(|&msg| ids.contains(&msg.get_id()))
                .cloned()
                .collect();
        }
        ids.iter()
            .filter_map(|id| self.events.get(id))
            .cloned()
            .collect()
    }

    pub fn get_event(&self, id: &U256) -> Option<Event> {
        self.get_events_by_ids(vec![*id]).into_iter().next()
    }

    // Ensures that there are no more than max_events stored.
    // Deletes the oldest messages if there are more.
    fn limit(&mut self) {
        if self.events.len() <= self.config.max_events {
            return;
        }
        let ids: Vec<U256> = self
            .events
            .iter()
            .map(|(k, v)| (k, v.created))
            .sorted_by(|a, b| b.1.partial_cmp(&a.1).unwrap())
            .skip(self.config.max_events)
            .map(|(k, _)| k)
            .cloned()
            .collect();
        for id in ids {
            self.events.remove(&id);
        }
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Category {
    TextMessage,
    NodeInfo,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct CategoryConfig {
    // Only one event per node
    unique: bool,
    // How many events to hold
    max_events: usize,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Event {
    pub category: Category,
    pub src: U256,
    pub created: f64,
    pub msg: String,
}

impl Event {
    pub fn get_id(&self) -> U256 {
        let mut id = Sha256::new();
        id.update(format!("{:?}", self.category));
        id.update(self.src);
        id.update(self.created.to_le_bytes());
        id.update(&self.msg);
        id.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_double_unique(){
        let mut evs = Events{
            config: CategoryConfig{ unique: true, max_events: 10 },
            events: HashMap::new(),
        };

        let e1 = Event{ category: Category::NodeInfo, src: U256::rnd(), created: 1., msg: "foo".into() };
        let e2 = Event{ category: Category::NodeInfo, src: e1.src, created: 2., msg: "bar".into() };

        assert_eq!(0, evs.events.len());
        assert!(evs.insert(e1.clone()));
        assert_eq!(1, evs.events.len());
        assert!(!evs.insert(e1));
        assert_eq!(1, evs.events.len());
        assert!(evs.insert(e2.clone()));
        assert_eq!(1, evs.events.len());
        assert!(evs.events.get(&e2.get_id()).is_some());
    }

    #[test]
    fn test_double_normal(){
        let mut evs = Events{
            config: CategoryConfig{ unique: false, max_events: 10 },
            events: HashMap::new(),
        };

        let e1 = Event{ category: Category::NodeInfo, src: U256::rnd(), created: 1., msg: "foo".into() };
        let e2 = Event{ category: Category::NodeInfo, src: e1.src, created: 2., msg: "bar".into() };

        assert_eq!(0, evs.events.len());
        assert!(evs.insert(e1.clone()));
        assert_eq!(1, evs.events.len());
        assert!(!evs.insert(e1.clone()));
        assert_eq!(1, evs.events.len());
        assert!(evs.insert(e2.clone()));
        assert_eq!(2, evs.events.len());
        assert!(evs.events.get(&e1.get_id()).is_some());
        assert!(evs.events.get(&e2.get_id()).is_some());
    }
}