//! # Network related structures
//!
//! The basic structure here is [`Network`], which handles all communications
//! with the signalling server and setting up of new WebRTC connections.
//! As the [`Network`] simply returns a [`Broker<NetworkMessage>`], a more
//! user-friendly wrapper named [`NetworkWebRTC`] exists, which is better suited for
//! usage in a non-[`Broker`] world.
//!
//! Both of these structures are best created with [`crate::network_start`] and
//! [`crate::network_broker_start`].

use core::panic;
use std::{fmt, time::Duration};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{mpsc::UnboundedReceiver, watch};
use tokio_stream::StreamExt;

use flarch::{
    broker::{Broker, BrokerError, TranslateFrom, TranslateInto},
    nodeids::NodeID,
    tasks::Interval,
    web_rtc::{
        messages::{ConnType, SetupError, SignalingState},
        node_connection::{Direction, NCError},
        websocket::{BrokerWSClient, WSClientIn, WSClientOut},
        BrokerWebRTCConn, WebRTCConnInput, WebRTCConnOutput,
    },
};

use crate::{
    network::signal::NodeStat,
    nodeconfig::{NodeConfig, NodeInfo},
    router::messages::NetworkWrapper,
    timer::Timer,
};

use super::{
    messages::{InternIn, InternOut, Messages},
    signal::FledgerConfig,
};

pub type BrokerNetwork = Broker<NetworkIn, NetworkOut>;

pub const MODULE_NAME: &str = "Network";

#[derive(Debug, Serialize, Deserialize)]
pub enum NetworkModule {
    String(String),
    Signal,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
/// These are similar to public methods on a structure.
/// Sending these messages will call the linked actions.
pub enum NetworkIn {
    /// Sends a new text message to the node.
    /// The [`Network`] will try to set up a connection with the remote node,
    /// if no such connection exists yet.
    /// If the node is not connected to the signalling handler, nothing happens.
    MessageToNode(NodeID, NetworkWrapper),
    /// Sends some stats to the signalling server to monitor the overall health of
    /// the system.
    StatsToWS(Vec<NodeStat>),
    /// Requests a new list of currenlty connected nodes to the signalling server.
    WSUpdateListRequest,
    /// Connect to the given node.
    /// If the node is not connected to the signalling server, no connection is made,
    /// and no error is produced.
    Connect(NodeID),
    /// Manually disconnect from the given node.
    /// If there is no connection to this node, no error is produced.
    Disconnect(NodeID),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
/// Messages sent from the [`Network`] to the user.
pub enum NetworkOut {
    /// A new message has been received from the given node.
    MessageFromNode(NodeID, NetworkWrapper),
    /// An updated list coming from the signalling server.
    NodeListFromWS(Vec<NodeInfo>),
    /// Whenever the state of a connection changes, this message is
    /// sent to the user.
    ConnectionState(NetworkConnectionState),
    /// A node has been successfully connected.
    Connected(NodeID),
    /// A node has been disconnected.
    Disconnected(NodeID),
    /// Configuration from the signalling server
    SystemConfig(FledgerConfig),
}

#[derive(Error, Debug)]
/// Possible errors from the [`Network`].
pub enum NetworkError {
    #[error("Connection not found")]
    ConnectionMissing,
    #[error("Cannot connect to myself")]
    ConnectMyself,
    #[error("Signalling server doesn't respond")]
    SignallingServer,
    #[error(transparent)]
    SerdeJSON(#[from] serde_json::Error),
    #[error(transparent)]
    NodeConnection(#[from] NCError),
    #[error(transparent)]
    Broker(#[from] BrokerError),
    #[error(transparent)]
    Setup(#[from] SetupError),
}

/// The [`Network`] handles setting up webRTC connections for all connected nodes.
/// It can handle incoming and outgoing connections.
///
/// In this version, all connection setup (signalling) is going through a websocket.
/// In a next version, it should also be possible to setup new connections (signalling) through existing WebRTC
/// connections: If A and B are connected, and B and C are connected, C can connect to A by using
/// B as a signalling server.
pub struct Network {
    pub broker: BrokerNetwork,
    pub connections: watch::Receiver<Vec<NodeID>>,
}

impl Network {
    /// Starts a new [`Network`] and returns a [`BrokerNetwork`] which can be linked
    /// to other brokers.
    /// If you just need a simple send/receive interface, use the [`NetworkWebRTC`].
    pub async fn start(
        node_config: NodeConfig,
        ws: BrokerWSClient,
        web_rtc: BrokerWebRTCConn,
        timer: &mut Timer,
    ) -> anyhow::Result<Self> {
        let mut intern = Broker::new();
        let (messages, connections) = Messages::start(node_config);
        intern.add_handler(Box::new(messages)).await?;
        intern.link_bi(ws).await?;
        intern.link_bi(web_rtc).await?;
        let broker = Broker::new();
        intern.link_direct(broker.clone()).await?;
        timer.tick_second(intern, InternIn::Tick).await?;

        Ok(Self {
            broker,
            connections,
        })
    }
}

/// This is a user-friendly version of [`Network`].
/// Upon starting, it waits for the connection to the signalling server.
/// It has a simple API to send and receive messages to other nodes.
pub struct NetworkWebRTC {
    broker_net: BrokerNetwork,
    tap: UnboundedReceiver<NetworkOut>,
}

impl NetworkWebRTC {
    /// Takes a [`Network`] and returns the wrapped [`Network`].
    /// It waits for the [`Network`] to be completely set up.
    /// For this reason the [`Network`] must not have been used before.
    pub async fn start(mut broker_net: BrokerNetwork) -> anyhow::Result<Self> {
        let (mut tap, _) = broker_net.get_tap_out().await?;
        let mut timeout = Interval::new_interval(Duration::from_secs(10));
        timeout.next().await;
        loop {
            tokio::select! {
                _ = timeout.next() => {
                    return Err(NetworkError::SignallingServer.into());
                }
                msg = tap.recv() => {
                    if matches!(msg, Some(NetworkOut::NodeListFromWS(_))){
                        break;
                    }
                }
            }
        }
        Ok(Self { broker_net, tap })
    }

    /// Wait for a message to be received from the network.
    /// This method waits for a [`NetworkOut`] message to be received, which
    /// are the only messages interesting for a user.
    pub async fn recv(&mut self) -> NetworkOut {
        loop {
            let msg = self.tap.recv().await;
            if let Some(msg_reply) = msg {
                return msg_reply;
            }
        }
    }

    /// Send a message to the [`Network`] asynchronously.
    /// The message is of type [`NetworkIn`], as this is what the user can
    /// send to the [`Network`].
    pub fn send(&mut self, msg: NetworkIn) -> anyhow::Result<()> {
        self.broker_net.emit_msg_in(msg)
    }

    /// Tries to send a text-message to a remote node.
    /// The [`Network`] will start a connection with the node if there is none available.
    /// If the remote node is not available, no error is returned.
    pub fn send_str(&mut self, dst: NodeID, msg: String) -> anyhow::Result<()> {
        self.send(NetworkIn::MessageToNode(
            dst,
            NetworkWrapper::wrap_yaml(MODULE_NAME, &NetworkModule::String(msg))?,
        ))
    }

    /// Requests an updated list of all connected nodes to the signalling server.
    pub fn send_list_request(&mut self) -> anyhow::Result<()> {
        self.send(NetworkIn::WSUpdateListRequest)
    }
}

impl TranslateFrom<WSClientOut> for InternIn {
    fn translate(msg: WSClientOut) -> Option<Self> {
        Some(InternIn::WebSocket(msg))
    }
}
impl TranslateInto<WSClientIn> for InternOut {
    fn translate(self) -> Option<WSClientIn> {
        match self {
            InternOut::WebSocket(msg) => Some(msg),
            _ => None,
        }
    }
}

/*
the trait bound `network::messages::InternIn: TranslateFrom<NetworkIn>` is not satisfied
the trait bound `network::messages::InternOut: TranslateInto<NetworkOut>` is not satisfied
 */

impl TranslateFrom<NetworkIn> for InternIn {
    fn translate(msg: NetworkIn) -> Option<Self> {
        Some(InternIn::Network(msg))
    }
}
impl TranslateInto<NetworkOut> for InternOut {
    fn translate(self) -> Option<NetworkOut> {
        match self {
            InternOut::Network(network_out) => Some(network_out),
            _ => None,
        }
    }
}

impl TranslateFrom<WebRTCConnOutput> for InternIn {
    fn translate(msg: WebRTCConnOutput) -> Option<Self> {
        Some(InternIn::WebRTC(msg))
    }
}

impl TranslateInto<WebRTCConnInput> for InternOut {
    fn translate(self) -> Option<WebRTCConnInput> {
        match self {
            InternOut::WebRTC(msg_webrtc) => Some(msg_webrtc),
            _ => None,
        }
    }
}

impl fmt::Display for NetworkIn {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NetworkIn::MessageToNode(_, _) => write!(f, "MessageToNode()"),
            NetworkIn::StatsToWS(_) => write!(f, "StatsToWS()"),
            NetworkIn::WSUpdateListRequest => write!(f, "WSUpdateListRequest"),
            NetworkIn::Connect(_) => write!(f, "Connect()"),
            NetworkIn::Disconnect(_) => write!(f, "Disconnect()"),
        }
    }
}

impl fmt::Display for NetworkOut {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NetworkOut::MessageFromNode(_, _) => write!(f, "MessageFromNode()"),
            NetworkOut::NodeListFromWS(_) => write!(f, "NodeListFromWS()"),
            NetworkOut::ConnectionState(_) => write!(f, "ConnectionState()"),
            NetworkOut::Connected(_) => write!(f, "Connected()"),
            NetworkOut::Disconnected(_) => write!(f, "Disconnected()"),
            NetworkOut::SystemConfig(_) => write!(f, "SystemConfig"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
/// The connection state of a remote node.
pub struct NetworkConnectionState {
    /// The ID of the remote node.
    pub id: NodeID,
    /// The direction of the connection.
    pub dir: Direction,
    /// Statistics on the connection.
    pub s: ConnStats,
}

#[derive(Debug, Clone, PartialEq)]
/// Some statistics on the connection.
/// Not all fields are usable in all implementations.
pub struct ConnStats {
    /// What connection type the local end is having.
    pub type_local: ConnType,
    /// What connection tye the remote end is having.
    pub type_remote: ConnType,
    /// The current signalling state.
    pub signaling: SignalingState,
    /// Received bytes
    pub rx_bytes: u64,
    /// Transmitted bytes
    pub tx_bytes: u64,
    /// Round-trip time of the connection
    pub delay_ms: u32,
}

#[cfg(test)]
mod tests {
    use flarch::{nodeids::U256, start_logging};

    use crate::network::signal::WSSignalMessageToNode;

    #[test]
    fn test_serialize() -> anyhow::Result<()> {
        start_logging();

        let cha = U256::rnd();
        let msg = WSSignalMessageToNode::Challenge(2, cha);
        let msg_str = serde_json::to_string(&msg)?;
        log::debug!("Message string is: {msg_str}");

        let msg_clone = serde_json::from_str(&msg_str)?;
        assert_eq!(msg, msg_clone);

        Ok(())
    }
}
