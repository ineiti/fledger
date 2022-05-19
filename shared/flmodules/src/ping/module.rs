use flutils::nodeids::{NodeID, NodeIDs};
use serde::{Deserialize, Serialize};

use super::storage::PingStorage;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageNode {
    Ping,
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PingMessage {
    Input(PingIn),
    Output(PingOut),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PingIn {
    Tick,
    Message((NodeID, MessageNode)),
    NodeList(NodeIDs),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PingOut {
    Message((NodeID, MessageNode)),
    Storage(PingStorage),
    Failed(NodeID),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PingConfig {
    // How many ticks between two pings
    pub interval: u16,
    // How many ticks before a missing ping is counted as disconnection
    pub timeout: u16,
}

pub struct Ping {
    pub storage: PingStorage,
}

impl Ping {
    pub fn new(config: PingConfig) -> Self {
        Self {
            storage: PingStorage::new(config),
        }
    }

    pub fn process_msg(&mut self, msg: PingIn) -> Vec<PingOut> {
        match msg {
            PingIn::Tick => self.tick(),
            PingIn::Message((id, msg_node)) => self.message(id, msg_node),
            PingIn::NodeList(ids) => self.new_nodes(ids),
        }
    }

    pub fn tick(&mut self) -> Vec<PingOut> {
        self.storage.tick();
        itertools::concat([
            self.create_messages(),
            vec![PingOut::Storage(self.storage.clone())],
        ])
    }

    pub fn message(&mut self, id: NodeID, msg: MessageNode) -> Vec<PingOut> {
        match msg {
            MessageNode::Ping => {
                vec![PingOut::Message((id, MessageNode::Pong))]
            }
            MessageNode::Pong => {
                self.storage.pong(id);
                self.create_messages()
            }
        }
    }

    pub fn new_nodes(&mut self, ids: NodeIDs) -> Vec<PingOut> {
        for id in ids.0 {
            self.storage.new_node(id);
        }
        self.create_messages()
    }

    fn create_messages(&mut self) -> Vec<PingOut> {
        let mut out = vec![];
        for id in self.storage.ping.drain(..) {
            out.push(PingOut::Message((id, MessageNode::Ping)).into());
        }
        for id in self.storage.failed.drain(..) {
            out.push(PingOut::Failed(id).into());
        }

        out
    }
}

impl From<PingIn> for PingMessage {
    fn from(msg: PingIn) -> Self {
        PingMessage::Input(msg)
    }
}

impl From<PingOut> for PingMessage {
    fn from(msg: PingOut) -> Self {
        PingMessage::Output(msg)
    }
}

impl Default for PingConfig {
    fn default() -> Self {
        Self {
            interval: 5,
            timeout: 10,
        }
    }
}
