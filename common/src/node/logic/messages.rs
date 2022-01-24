use crate::node::{modules::gossip_chat, network::BrokerNetwork};
use serde::{Deserialize, Serialize};
use std::fmt;
use types::nodeids::U256;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
/// This is the command-set for version 1 of the protocol.
pub enum MessageV1 {
    // The version of the requested node
    Version(String),
    // Received command OK
    Ok(),
    // Error from one of the requests
    Error(String),
    // Pings an OracleServer
    Ping(),
    // GossipChat messages
    GossipChat(gossip_chat::MessageNode),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum Message {
    /// Version1 of messages - new messages can be added, but changes to existing
    /// types need to be backwards-compatible
    V1(MessageV1),
    /// Unknown message
    Unknown(String),
}

impl fmt::Display for MessageV1 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MessageV1::Version(_) => write!(f, "Version"),
            MessageV1::Ok() => write!(f, "Ok"),
            MessageV1::Error(_) => write!(f, "Error"),
            MessageV1::Ping() => write!(f, "Ping"),
            MessageV1::GossipChat(_) => write!(f, "GossipChat"),
        }
    }
}

impl From<&str> for Message {
    fn from(s: &str) -> Self {
        match serde_json::from_str(s) as Result<Message, serde_json::Error> {
            Ok(msg) => msg,
            Err(_) => Message::Unknown(s.to_string()),
        }
    }
}

impl From<String> for Message {
    fn from(s: String) -> Self {
        match serde_json::from_str(&s) as Result<Message, serde_json::Error> {
            Ok(msg) => msg,
            Err(_) => Message::Unknown(s),
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct NodeMessage {
    pub id: U256,
    pub msg: Message,
}

impl From<NodeMessage> for BrokerNetwork {
    fn from(msg: NodeMessage) -> Self {
        Self::NodeMessageOut(msg)
    }
}

impl fmt::Debug for NodeMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&format!(
            "NodeMessage( id: {}, msg: {:?}",
            self.id.short(),
            self.msg
        ))
    }
}
