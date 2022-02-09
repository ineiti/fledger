use async_trait::async_trait;
use ed25519_dalek::Signature;
use serde::{Deserialize, Serialize};
use std::fmt::{self};
use thiserror::Error;

use flutils::nodeids::U256;

use crate::{config::NodeInfo, network::BrokerNetworkCall};

#[derive(Debug, Error)]
pub enum SetupError {
    #[error("Spawning failed: {0}")]
    SpawnFail(String),
    #[error("Couldn't setup: {0}")]
    SetupFail(String),
    #[error("Invalid state: {0}")]
    InvalidState(String),
    #[error("From underlying system: {0}")]
    Underlying(String),
}

pub type WebRTCSpawner =
    Box<dyn Fn(WebRTCConnectionState) -> Result<Box<dyn WebRTCConnectionSetup>, SetupError>>;

pub type WebRTCSetupCB = Box<dyn FnMut(WebRTCSetupCBMessage) + Send>;

#[async_trait(?Send)]
pub trait WebRTCConnectionSetup {
    /// Returns the offer string that needs to be sent to the `Follower` node.
    async fn make_offer(&mut self) -> Result<String, SetupError>;

    /// Takes the offer string
    async fn make_answer(&mut self, offer: String) -> Result<String, SetupError>;

    /// Takes the answer string and finalizes the first part of the connection.
    async fn use_answer(&mut self, answer: String) -> Result<(), SetupError>;

    /// Returns either an ice string or a connection
    async fn set_callback(&mut self, cb: WebRTCSetupCB);

    /// Sends the ICE string to the WebRTC.
    async fn ice_put(&mut self, ice: String) -> Result<(), SetupError>;

    /// Return some statistics on the connection
    async fn get_state(&self) -> Result<ConnectionStateMap, SetupError>;
}

pub enum WebRTCSetupCBMessage {
    Ice(String),
    Connection(Box<dyn WebRTCConnection + Send>),
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

#[derive(Error, Debug)]
pub enum ConnectionError {
    #[error("Sending failed: {0}")]
    FailSend(String),
    #[error("Unknown State: {0}")]
    UnknownState(String),
    #[error("In underlying system: {0}")]
    Underlying(String),
}

#[async_trait(?Send)]
pub trait WebRTCConnection {
    /// Send a message to the other node. This call blocks until the message
    /// is queued.
    fn send(&self, s: String) -> Result<(), ConnectionError>;

    /// Sets the callback for incoming messages.
    fn set_cb_message(&self, cb: WebRTCMessageCB);

    /// Return some statistics on the connection
    async fn get_state(&self) -> Result<ConnectionStateMap, ConnectionError>;
}

#[derive(PartialEq, Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ConnType {
    Unknown,
    Host,
    STUNPeer,
    STUNServer,
    TURN,
}

#[derive(PartialEq, Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SignalingState {
    Closed,
    Setup,
    Stable,
}

/// This is copied from the web-sys RtcIceGatheringState - not sure that this is
/// also available in the libc-implementation.
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum IceGatheringState {
    New,
    Gathering,
    Complete,
}

/// This is copied from the web-sys RtcIceConnectionState - not sure that this is
/// also available in the libc-implementation.
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum IceConnectionState {
    New,
    Checking,
    Connected,
    Completed,
    Failed,
    Disconnected,
    Closed,
}

/// This is copied from the web-sys RtcDataChannelState - not sure that this is
/// also available in the libc-implementation.
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum DataChannelState {
    Connecting,
    Open,
    Closing,
    Closed,
}

/// Some statistics about the connection
#[derive(PartialEq, Debug, Clone, Copy)]
pub struct ConnectionStateMap {
    pub type_local: ConnType,
    pub type_remote: ConnType,
    pub signaling: SignalingState,
    pub ice_gathering: IceGatheringState,
    pub ice_connection: IceConnectionState,
    pub data_connection: Option<DataChannelState>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub delay_ms: u32,
}

pub type WebRTCMessageCB = Box<dyn FnMut(String) + Send>;

/// What type of node this is
#[derive(PartialEq, Debug, Clone, Copy, Serialize, Deserialize)]
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

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct PeerInfo {
    pub id_init: U256,
    pub id_follow: U256,
    pub message: PeerMessage,
}

impl std::fmt::Display for PeerInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "init: {} - follow: {} - msg: {}",
            self.id_init, self.id_follow, self.message
        )
    }
}

impl PeerInfo {
    pub fn new(init: &U256, follow: &U256) -> PeerInfo {
        PeerInfo {
            id_init: *init,
            id_follow: *follow,
            message: PeerMessage::Init,
        }
    }

    pub fn get_remote(&self, local: &U256) -> Option<U256> {
        if self.id_init == *local {
            return Some(self.id_follow);
        }
        if self.id_follow == *local {
            return Some(self.id_init);
        }
        None
    }

    pub fn send(self) -> BrokerNetworkCall {
        BrokerNetworkCall::SendWSPeer(self)
    }

    // pub fn receive(self) -> BrokerNetworkReply {
    //     BrokerNetworkReply::PeerSetup(self)
    // }
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
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub enum WSSignalMessageToNode {
    Challenge(u64, U256),
    ListIDsReply(Vec<NodeInfo>),
    PeerSetup(PeerInfo),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub enum WSSignalMessageFromNode {
    Announce(MessageAnnounce),
    ListIDsRequest,
    ClearNodes,
    PeerSetup(PeerInfo),
    NodeStats(Vec<NodeStat>),
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct NodeStat {
    pub id: U256,
    pub version: String,
    pub ping_ms: u32,
    pub ping_rx: u32,
}

impl std::fmt::Display for WSSignalMessageToNode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            WSSignalMessageToNode::Challenge(_, _) => write!(f, "Challenge"),
            WSSignalMessageToNode::ListIDsReply(_) => write!(f, "ListIDsReply"),
            WSSignalMessageToNode::PeerSetup(_) => write!(f, "PeerSetup"),
        }
    }
}

impl std::fmt::Display for WSSignalMessageFromNode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            WSSignalMessageFromNode::Announce(_) => write!(f, "Announce"),
            WSSignalMessageFromNode::ListIDsRequest => write!(f, "ListIDsRequest"),
            WSSignalMessageFromNode::PeerSetup(_) => write!(f, "PeerSetup"),
            WSSignalMessageFromNode::NodeStats(_) => write!(f, "NodeStats"),
            WSSignalMessageFromNode::ClearNodes => write!(f, "ClearNodes"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct MessageAnnounce {
    pub version: u64,
    pub challenge: U256,
    pub node_info: NodeInfo,
    pub signature: Signature,
}
