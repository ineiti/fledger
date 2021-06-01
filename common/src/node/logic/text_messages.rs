use std::{collections::HashMap, sync::mpsc::Sender};

/// TextMessages is the structure that holds all known published TextMessages.
use log::info;
use sha2::{Digest, Sha256};

use crate::{node::config::{NodeConfig, NodeInfo}, types::U256};
use serde::{Deserialize, Serialize};

use super::{messages::MessageReplyV1, LOutput};

pub struct TextMessages {
    messages: HashMap<U256, TextMessage>,
    out: Sender<LOutput>,
    nodes_msgs: HashMap<U256, Vec<U256>>,
}

impl TextMessages {
    pub fn new(out: Sender<LOutput>, cfg: NodeConfig) -> Self {
        Self {
            messages: HashMap::new(),
            nodes_msgs: HashMap::new(),
            out,
        }
    }

    pub fn handle_send(&mut self, msg: SendMessagesV1) -> Result<Option<ReplyMessagesV1>, String> {
        match msg {
            SendMessagesV1::List() => {
                let ids = vec![];
                Ok(Some(ReplyMessagesV1::IDs(ids)))
            }
            SendMessagesV1::Get(id) => {
                if let Some(text) = self.messages.get(&id) {
                    Ok(Some(ReplyMessagesV1::Text(text.clone())))
                } else {
                    Err("don't know this message".to_string())
                }
            }
            SendMessagesV1::Set(text) => {
                self.messages.insert(text.id(), text);
                Ok(None)
            }
        }
    }

    pub fn handle_reply(&mut self, from: &U256, msg: ReplyMessagesV1) -> Result<(), String> {
        match msg {
            // Currently we just suppose that there is a central node storing all node-ids
            // and texts, so we can simply overwrite the ones we already have.
            ReplyMessagesV1::IDs(list) => {}
            ReplyMessagesV1::Text(_) => {}
        }
        Ok(())
    }

    /// Updates all known nodes. Will send out requests to new nodes to store what
    /// messages are available in those nodes.
    pub fn update_nodes(&mut self, nodes: Vec<NodeInfo>) {
        
    }

    fn send<T: Serialize>(&self, to: &U256, msg: &T) -> Result<(), String> {
        let str = serde_json::to_string(msg).map_err(|e| e.to_string())?;
        self.out
            .send(LOutput::WebRTC(to.clone(), str))
            .map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SendMessagesV1 {
    // Requests the updated list of all TextIDs available. This is best
    // sent to one of the Oracle Servers.
    List(),
    // Request a text from a node. If the node doesn't have this text
    // available, it should respond with an Error.
    Get(U256),
    // Stores a new text on the Oracle Servers
    Set(TextMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReplyMessagesV1 {
    // Tuples of [NodeID ; TextID] indicating texts and where they should
    // be read from.
    // The NodeID can change from one TextIDsGet call to another, as nodes
    // can be coming and going.
    IDs(Vec<TextStorage>),
    // The Text as known by the node.
    Text(TextMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextStorage {
    pub id: U256,
    pub nodes: Vec<U256>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextMessage {
    pub src: U256,
    pub created: u64,
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

#[cfg(test)]
mod tests {
    use super::TextMessage;
    use crate::types::U256;

    #[test]
    fn test_id() {
        let tm1 = TextMessage {
            src: U256::rnd(),
            created: 0u64,
            liked: 0u64,
            msg: "test message".to_string(),
        };
        assert_eq!(tm1.id(), tm1.id());

        let mut tm2 = tm1.clone();
        tm2.src = U256::rnd();
        assert_ne!(tm1.id(), tm2.id());

        tm2 = tm1.clone();
        tm2.created = 1u64;
        assert_ne!(tm1.id(), tm2.id());

        tm2 = tm1.clone();
        tm2.liked = 1u64;
        assert_ne!(tm1.id(), tm2.id());

        tm2 = tm1.clone();
        tm2.msg = "short test".to_string();
        assert_ne!(tm1.id(), tm2.id());
    }
}
