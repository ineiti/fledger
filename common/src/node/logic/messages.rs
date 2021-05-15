use crate::types::U256;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize, Deserialize)]
pub struct TextMessage {
    pub src: U256,
    pub created: u64,
    pub liked: u32,
    pub msg: String,
}

impl TextMessage {
    pub fn id(&self) -> U256 {
        let mut id = Sha256::new();
        id.update(&self.src);
        id.finalize().into()
    }
}

#[derive(Debug, Serialize, Deserialize)]
/// This is the command-set for version 1 of the protocol.
pub enum MessageSendV1 {
    // Pings an OracleServer
    Ping(),
    // Requests the updated list of all TextIDs available. This is best
    // sent to one of the Oracle Servers.
    TextIDsGet(),
    // Request a text from a node. If the node doesn't have this text
    // available, it should respond with an Error.
    TextGet(U256),
    // Stores a new text on the Oracle Servers
    TextSet(TextMessage),
}

#[derive(Debug, Serialize, Deserialize)]
/// This is the command-set for version 1 of the protocol.
pub enum MessageReplyV1 {
    // Tuples of [NodeID ; TextID] indicating texts and where they should
    // be read from.
    // The NodeID can change from one TextIDsGet call to another, as nodes
    // can be coming and going.
    TextIDs(Vec<[U256; 2]>),
    // The Text as known by the node.
    Text(TextMessage),
    // Received command OK
    Ok(),
    // Error from one of the requests
    Error(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    /// Used to send a message to another node.
    /// The U256 is used as an index to identify the message.
    SendV1((U256, MessageSendV1)),
    /// The reply to the messages.
    ReplyV1((U256, MessageReplyV1)),
    /// Unknown message
    Unknown(String),
}

impl From<&str> for Message {
    fn from(s: &str) -> Self {
        match serde_json::from_str(s) as Result<Message, serde_json::Error> {
            Ok(msg) => msg,
            Err(_) => Message::Unknown(s.to_string())
        }
    }
}

impl From<String> for Message {
    fn from(s: String) -> Self {
        match serde_json::from_str(&s) as Result<Message, serde_json::Error> {
            Ok(msg) => msg,
            Err(_) => Message::Unknown(s)
        }
    }
}

impl Message {
    pub fn to_string(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| e.to_string())
    }
}