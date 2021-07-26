use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
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
    // Gets the Version of the OracleServers
    VersionGet(),
    // TextMessages
    TextMessage(super::text_messages::TextMessageV1),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    /// Version1 of messages - new messages can be added, but changes to existing
    /// types need to be backwards-compatible
    V1(MessageV1),
    /// Unknown message
    Unknown(String),
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

impl Message {
    pub fn to_string(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| e.to_string())
    }
}
