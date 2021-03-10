use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::node::{config::NodeInfo, types::U256};

pub type WebRTCSpawner =
    Box<dyn Fn(WebRTCConnectionState) -> Result<Box<dyn WebRTCConnectionSetup>, String>>;

pub type WebRTCSetupCB = Box<dyn Fn(WebRTCSetupCBMessage)>;

#[async_trait(?Send)]
pub trait WebRTCConnectionSetup {
    /// Returns the offer string that needs to be sent to the `Follower` node.
    async fn make_offer(&mut self) -> Result<String, String>;

    /// Takes the offer string
    async fn make_answer(&mut self, offer: String) -> Result<String, String>;

    /// Takes the answer string and finalizes the first part of the connection.
    async fn use_answer(&mut self, answer: String) -> Result<(), String>;

    /// Returns either an ice string or a connection
    async fn set_callback(&mut self, cb: WebRTCSetupCB);

    /// Sends the ICE string to the WebRTC.
    async fn ice_put(&mut self, ice: String) -> Result<(), String>;

    /// TODO: this can probably go away
    async fn wait_gathering(&mut self) -> Result<(), String>;

    /// Debugging output of the RTC state
    async fn print_states(&mut self);
}

pub enum WebRTCSetupCBMessage {
    Ice(String),
    Connection(Box<dyn WebRTCConnection>),
}

impl fmt::Debug for WebRTCSetupCBMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WebRTCSetupCBMessage::Ice(_) => f.write_str("Ice")?,
            WebRTCSetupCBMessage::Connection(_) => f.write_str("Connection")?,
        }
        Ok(())
    }
}


#[async_trait(?Send)]
pub trait WebRTCConnection {
    /// Send a message to the other node. This call blocks until the message
    /// is queued.
    fn send(&self, s: String) -> Result<(), String>;

    /// Sets the callback for incoming messages.
    fn set_cb_message(&self, cb: WebRTCMessageCB);

    /// Return some statistics on the connection
    async fn get_state(&self) -> Result<ConnectionStateMap, String>;
}

/// Some statistics about the connection
#[derive(PartialEq, Debug, Clone, Copy)]
pub struct ConnectionStateMap {
    pub stun_local: bool,
    pub stun_remote: bool,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub delay_ms: u32,
}

pub type WebRTCMessageCB = Box<dyn FnMut(String)>;

/// What type of node this is
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum WebRTCConnectionState {
    Initializer,
    Follower,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub enum PeerMessage {
    Init,
    Offer(String),
    Answer(String),
    IceCandidate(String),
}

impl std::fmt::Display for PeerMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            PeerMessage::Init => write!(f, "Init"),
            PeerMessage::Offer(_) => write!(f, "Offer"),
            PeerMessage::Answer(_) => write!(f, "Answer"),
            PeerMessage::IceCandidate(_) => write!(f, "IceCandidate"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PeerInfo {
    pub id_init: U256,
    pub id_follow: U256,
    pub message: PeerMessage,
}

impl PeerInfo {
    pub fn new(init: &U256, follow: &U256) -> PeerInfo {
        PeerInfo {
            id_init: init.clone(),
            id_follow: follow.clone(),
            message: PeerMessage::Init,
        }
    }

    pub fn get_remote(&self, local: &U256) -> Option<U256> {
        if self.id_init == local.clone() {
            return Some(self.id_follow.clone());
        }
        if self.id_follow == local.clone() {
            return Some(self.id_init.clone());
        }
        return None;
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WebSocketMessage {
    pub msg: WSSignalMessage,
}

impl WebSocketMessage {
    pub fn from_str(s: &str) -> Result<WebSocketMessage, String> {
        serde_json::from_str(s).map_err(|err| err.to_string())
    }

    pub fn to_string(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

/// Message is a list of messages to be sent between the node and the signal server.
/// When a new node connects to the signalling server, the server starts by sending
/// a "Challenge" to the node.
/// The node can then announce itself using that challenge.
/// - ListIDs* are used by the nodes to get a list of currently connected nodes
/// - ClearNodes is a debugging message that will be removed at a later stage.
/// - PeerRequest is sent by a node to ask to connect to another node. The
/// server will send a 'PeerReply' to the corresponding node, which will continue
/// the protocol by sending its own PeerRequest.
/// - Done is a standard message that can be sent back to indicate all is well.
///
/// TODO: use the "Challenge" to sign with the private key of the node, so that the server
/// can verify that the node knows the corresponding private key of its public key.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum WSSignalMessage {
    Challenge(U256),
    Announce(MessageAnnounce),
    ListIDsRequest,
    ListIDsReply(Vec<NodeInfo>),
    ClearNodes,
    PeerSetup(PeerInfo),
    Done,
}

impl std::fmt::Display for WSSignalMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            WSSignalMessage::Challenge(_) => write!(f, "Challenge"),
            WSSignalMessage::Announce(_) => write!(f, "Announce"),
            WSSignalMessage::ListIDsRequest => write!(f, "ListIDsRequest"),
            WSSignalMessage::ListIDsReply(_) => write!(f, "ListIDsReply"),
            WSSignalMessage::ClearNodes => write!(f, "ClearNodes"),
            WSSignalMessage::PeerSetup(_) => write!(f, "PeerSetup"),
            WSSignalMessage::Done => write!(f, "Done"),
        }
    }
}

/// TODO: add a signature on the challenge
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MessageAnnounce {
    pub challenge: U256,
    pub node_info: NodeInfo,
}
