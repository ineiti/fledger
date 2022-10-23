use futures::{channel::oneshot::Canceled, Future};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::RecvError;
use thiserror::Error;

use flmodules::{broker::Broker, nodeids::U256};

use crate::network_broker::NetCall;

use super::node_connection::Direction;

#[derive(Debug, Error)]
pub enum SetupError {
    #[error("Couldn't setup: {0}")]
    SetupFail(String),
    #[error("Invalid state: {0}")]
    InvalidState(String),
    #[error("While sending: {0}")]
    Send(String),
    #[error(transparent)]
    Broker(#[from] flmodules::broker::BrokerError),
    #[error(transparent)]
    Recv(#[from] RecvError),
    #[error(transparent)]
    Canceled(#[from] Canceled),
}

#[derive(Debug, Clone, PartialEq)]
pub enum WebRTCMessage {
    Output(WebRTCOutput),
    Input(WebRTCInput),
}

#[derive(Debug, Clone, PartialEq)]
pub enum WebRTCOutput {
    Connected,
    Disconnected,
    Text(String),
    Setup(PeerMessage),
    State(ConnectionStateMap),
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum WebRTCInput {
    Text(String),
    Setup(PeerMessage),
    Flush,
    UpdateState,
    Reset,
}

#[cfg(not(feature = "nosend"))]
pub type WebRTCSpawner = Box<
    dyn Fn() -> Box<dyn Future<Output = Result<Broker<WebRTCMessage>, SetupError>> + Unpin + Send>
        + Send
        + Sync,
>;
#[cfg(feature = "nosend")]
pub type WebRTCSpawner =
    Box<dyn Fn() -> Box<dyn Future<Output = Result<Broker<WebRTCMessage>, SetupError>> + Unpin>>;

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

    pub fn get_direction(&self, local: &U256) -> Direction {
        if self.id_init == *local {
            return Direction::Outgoing;
        }
        return Direction::Incoming;
    }

    pub fn send(self) -> NetCall {
        NetCall::SendWSPeer(self)
    }

    // pub fn receive(self) -> BrokerNetworkReply {
    //     BrokerNetworkReply::PeerSetup(self)
    // }
}
