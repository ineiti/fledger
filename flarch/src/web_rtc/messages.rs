//! # Messages for WebRTC connections
//!
//! This is a list of messages that are used by the [`crate::web_rtc::WebRTCConn`] and
//! [`crate::web_rtc::node_connection::NodeConnection`] brokers to communicate with the libc and wasm implementations
//! of the WebRTC subsystem.
//! Both the wasm and the libc implementation for the WebRTC system return a [`Broker<WebRTCMessage>`]
//! that is then given to the [`crate::web_rtc::WebRTCConn`].
use futures::{channel::oneshot::Canceled, Future};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::RecvError;
use thiserror::Error;

use crate::{broker::{Broker, BrokerError}, nodeids::NodeID};

use super::node_connection::Direction;

#[derive(Debug, Error)]
/// Error messages for failing setups
pub enum SetupError {
    /// WebRTC setup fail
    #[error("Couldn't setup: {0}")]
    SetupFail(String),
    /// Didn't receive an appropriate state from the signalling server / the remote node
    #[error("Invalid state: {0}")]
    InvalidState(String),
    /// Couldn't send over the network
    #[error("While sending: {0}")]
    Send(String),
    /// Some error from the broker subsystem
    #[error(transparent)]
    Broker(#[from] BrokerError),
    /// Couldn't receive a message
    #[error(transparent)]
    Recv(#[from] RecvError),
    /// Got cancelled
    #[error(transparent)]
    Canceled(#[from] Canceled),
}

#[derive(Debug, Clone, PartialEq)]
/// Messages used in the WebRTC subsystem.
pub enum WebRTCMessage {
    /// Sent from the WebRTC subsystem
    Output(WebRTCOutput),
    /// Commands for the WebRTC subsystem
    Input(WebRTCInput),
}

#[derive(Debug, Clone, PartialEq)]
/// Output messages for the WebRTC subsystem
pub enum WebRTCOutput {
    /// Successfully connected to a node
    Connected,
    /// Successfully disconnected to a node
    Disconnected,
    /// Received a message from a node
    Text(String),
    /// Setup message for setting up or maintaining a WebRTC connection
    Setup(PeerMessage),
    /// Current state of the connection
    State(ConnectionStateMap),
    /// Generic error
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
/// Command for the WebRTC subsystem
pub enum WebRTCInput {
    /// Send a text message
    Text(String),
    /// Treat a setup message
    Setup(PeerMessage),
    /// Flush all pending messages
    Flush,
    /// Send the current state of the connection
    UpdateState,
    /// Try to reconnect, or throw an error if incoming connection
    Reset,
    /// Disconnect this node
    Disconnect,
}

#[cfg(target_family="unix")]
/// The spawner will create a new [`Broker<WebRTCMessage>`] ready to handle either an incoing
/// or an outgoing connection.
pub type WebRTCSpawner = Box<
    dyn Fn() -> Box<dyn Future<Output = Result<Broker<WebRTCMessage>, SetupError>> + Unpin + Send>
        + Send
        + Sync,
>;
#[cfg(target_family="wasm")]
/// The spawner will create a new [`Broker<WebRTCMessage>`] ready to handle either an incoing
/// or an outgoing connection.
pub type WebRTCSpawner =
    Box<dyn Fn() -> Box<dyn Future<Output = Result<Broker<WebRTCMessage>, SetupError>> + Unpin>>;

#[derive(PartialEq, Debug, Clone, Copy, Serialize, Deserialize)]
/// How the current connection is being setup.
pub enum ConnType {
    /// No usable state from the subsystem
    Unknown,
    /// Direct connection to the host
    Host,
    /// Got connection details from a STUN server
    STUNPeer,
    /// Got connection details from a STUN server
    STUNServer,
    /// Connection going through a TURN server, so no direct node-to-node connection
    TURN,
}

#[derive(PartialEq, Debug, Clone, Copy, Serialize, Deserialize)]
/// In what signalling state the WebRTC subsystem is
pub enum SignalingState {
    /// Currently closed, no setup going on
    Closed,
    /// Setup started, but connection is not usable
    Setup,
    /// Connection is stable and can be used
    Stable,
}

/// This is copied from the web-sys RtcIceGatheringState - not sure that this is
/// also available in the libc-implementation.
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum IceGatheringState {
    /// Just started the connection
    New,
    /// Gathering first information
    Gathering,
    /// Enough information for connection setup
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

impl Default for ConnectionStateMap {
    fn default() -> Self {
        Self {
            type_local: ConnType::Unknown,
            type_remote: ConnType::Unknown,
            signaling: SignalingState::Closed,
            ice_gathering: IceGatheringState::New,
            ice_connection: IceConnectionState::New,
            data_connection: None,
            rx_bytes: 0,
            tx_bytes: 0,
            delay_ms: 0,
        }
    }
}

/// A setup message being sent between two nodes to setup or maintain
/// a WebRTC connection.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub enum PeerMessage {
    /// The receiver should start the connection by creating an offer
    Init,
    /// The receiver should use the given information to create an answer
    Offer(String),
    /// The receiver should use the given information and wait for enough [`PeerMessage::IceCandidate`]s to
    /// finalize the setup of the connection
    Answer(String),
    /// Detailed information about available ports, poked holes in the NAT, or any other information
    /// to connect two nodes over WebRTC
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

/// Data sent between two nodes using the signalling server to set up
/// a peer-to-peer connection over WebRTC.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct PeerInfo {
    /// The ID of the node sending the [`PeerMessage::Init`] message: the
    /// creator of the outgoing connection
    pub id_init: NodeID,
    /// The ID of the node receiving the [`PeerMessage::Init`] message: the
    /// creator of the incoming connection
    pub id_follow: NodeID,
    /// The actual data to be sent to the WebRTC subsystem
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
    /// Creates a new [`PeerInfo`] with an init message
    pub fn new(init: &NodeID, follow: &NodeID) -> PeerInfo {
        PeerInfo {
            id_init: *init,
            id_follow: *follow,
            message: PeerMessage::Init,
        }
    }

    /// Gets the other ID given one of the two IDs
    pub fn get_remote(&self, local: &NodeID) -> Option<NodeID> {
        if self.id_init == *local {
            return Some(self.id_follow);
        }
        if self.id_follow == *local {
            return Some(self.id_init);
        }
        None
    }

    /// Gets the direction of the setup given the local ID
    pub fn get_direction(&self, local: &NodeID) -> Direction {
        if self.id_init == *local {
            return Direction::Outgoing;
        }
        return Direction::Incoming;
    }
}
