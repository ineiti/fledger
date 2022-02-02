use flutils::nodeids::U256;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::{
    node::{
        modules::{gossip_events, gossip_events::GossipMessage, random_connections::RandomMessage},
        network::{BrokerNetwork, NetworkConnectionState},
        timer::BrokerTimer,
    },
    signal::web_rtc::{PeerInfo, WSSignalMessage},
};

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum BrokerMessage {
    Network(BrokerNetwork),
    Timer(BrokerTimer),
    Modules(BrokerModules),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BrokerModules {
    Gossip(GossipMessage),
    Random(RandomMessage),
}

impl std::fmt::Display for BrokerMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BrokerMessage({})",
            match self {
                BrokerMessage::Network(_) => "Network",
                BrokerMessage::Timer(_) => "Timer",
                BrokerMessage::Modules(_) => "Modules",
            }
        )
    }
}

impl From<BrokerModules> for BrokerMessage {
    fn from(msg: BrokerModules) -> Self {
        Self::Modules(msg)
    }
}

transitive_from::hierarchy! {
    BrokerMessage {
        BrokerNetwork {
            NetworkConnectionState,
            WSSignalMessage {
                PeerInfo
            }
        },
        BrokerTimer,
        BrokerModules {
            GossipMessage {
                flmodules::gossip_events::MessageIn,
                flmodules::gossip_events::MessageOut,
            },
            RandomMessage {
                // flmodules::random_connections::MessageIn,
                // flmodules::random_connections::MessageOut,
            },
        },
    }
}

#[derive(Clone, PartialEq)]
pub struct NodeMessage {
    pub id: U256,
    pub msg: Message,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum Message {
    /// Version1 of messages - new messages can be added, but changes to existing
    /// types need to be backwards-compatible
    V1(MessageV1),
    /// Unknown message
    Unknown(String),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
/// This is the command-set for version 1 of the protocol.
pub enum MessageV1 {
    // The version of the requested node
    Version(String),
    // Received command OK
    Ok(),
    // Error from one of the requests
    Error(String),
    // Pings another node
    Ping(),
    // GossipChat messages
    GossipChat(gossip_events::MessageNode),
}

impl NodeMessage {
    pub fn input(&self) -> BrokerMessage {
        BrokerNetwork::NodeMessageIn(self.clone()).into()
    }
    pub fn output(&self) -> BrokerMessage {
        BrokerNetwork::NodeMessageOut(self.clone()).into()
    }
}

impl fmt::Debug for NodeMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&format!(
            "NodeMessage( id: {}, msg: {:?}",
            self.id, self.msg
        ))
    }
}

impl From<MessageV1> for Message {
    fn from(msg: MessageV1) -> Self {
        Self::V1(msg)
    }
}

impl From<gossip_events::MessageNode> for MessageV1 {
    fn from(msg: gossip_events::MessageNode) -> Self {
        Self::GossipChat(msg)
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

transitive_from::hierarchy! {
    Message {
        MessageV1{
            gossip_events::MessageNode
        }
    }
}
