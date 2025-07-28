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
use itertools::concat;
use std::{fmt, time::Duration};
use thiserror::Error;
use tokio::sync::{mpsc::UnboundedReceiver, watch};
use tokio_stream::StreamExt;

use flarch::{
    broker::{Broker, BrokerError, SubsystemHandler, TranslateFrom, TranslateInto},
    nodeids::{NodeID, U256},
    platform_async_trait,
    tasks::Interval,
    web_rtc::{
        messages::{ConnType, PeerInfo, SetupError, SignalingState},
        node_connection::{Direction, NCError, NCInput, NCOutput},
        websocket::{BrokerWSClient, WSClientIn, WSClientOut},
        BrokerWebRTCConn, WebRTCConnInput, WebRTCConnOutput,
    },
};

use crate::{
    network::signal::{
        MessageAnnounce, NodeStat, WSSignalMessageFromNode, WSSignalMessageToNode, SIGNAL_VERSION,
    },
    nodeconfig::{NodeConfig, NodeInfo},
};

use super::signal::FledgerConfig;

pub type BrokerNetwork = Broker<NetworkIn, NetworkOut>;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
/// These are similar to public methods on a structure.
/// Sending these messages will call the linked actions.
pub enum NetworkIn {
    /// Sends a new text message to the node.
    /// The [`Network`] will try to set up a connection with the remote node,
    /// if no such connection exists yet.
    /// If the node is not connected to the signalling handler, nothing happens.
    MessageToNode(NodeID, String),
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
    /// This message should be sent once a second to allow calculations of timeouts.
    Tick,
    WebSocket(WSClientOut),
    WebRTC(WebRTCConnOutput),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
/// Messages sent from the [`Network`] to the user.
pub enum NetworkOut {
    /// A new message has been received from the given node.
    MessageFromNode(NodeID, String),
    /// An updated list coming from the signalling server.
    NodeListFromWS(Vec<NodeInfo>),
    /// Whenever the state of a connection changes, this message is
    /// sent to the user.
    ConnectionState(NetworkConnectionState),
    /// A node has been successfully connected.
    Connected(NodeID),
    /// A node has been disconnected.
    Disconnected(NodeID),
    /// This is used in the From and Into methods
    WebSocket(WSClientIn),
    WebRTC(WebRTCConnInput),
    /// Configuration from the signalling server
    SystemConfig(FledgerConfig),
    /// A fatal error happened
    Error(String),
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

const UPDATE_INTERVAL: usize = 10;

impl Network {
    /// Starts a new [`Network`] and returns a [`BrokerNetwork`] which can be linked
    /// to other brokers.
    /// If you just need a simple send/receive interface, use the [`NetworkWebRTC`].
    pub async fn start(
        node_config: NodeConfig,
        ws: BrokerWSClient,
        web_rtc: BrokerWebRTCConn,
    ) -> anyhow::Result<Self> {
        let mut broker = Broker::new();
        let (messages, connections) = Messages::start(node_config);
        broker.add_handler(Box::new(messages)).await?;

        broker.link_bi(ws).await?;
        broker.link_bi(web_rtc).await?;

        Ok(Self {
            broker,
            connections,
        })
    }
}

struct Messages {
    node_config: NodeConfig,
    get_update: usize,
    connections: Vec<NodeID>,
    tx: Option<watch::Sender<Vec<NodeID>>>,
}

impl Messages {
    pub fn start(node_config: NodeConfig) -> (Self, watch::Receiver<Vec<NodeID>>) {
        let (tx, rx) = watch::channel(vec![]);
        (
            Self {
                node_config,
                get_update: UPDATE_INTERVAL,
                connections: vec![],
                tx: Some(tx),
            },
            rx,
        )
    }

    /// Processes incoming messages from the signalling server.
    /// This can be either messages requested by this node, or connection
    /// setup requests from another node.
    async fn msg_ws(&mut self, msg: WSClientOut) -> Vec<NetworkOut> {
        let msg_node_str = match msg {
            WSClientOut::Message(msg) => msg,
            WSClientOut::Error(e) => {
                log::warn!("Websocket client error: {e}");
                return vec![];
            }
            _ => return vec![],
        };
        let msg_node =
            if let Ok(msg_node) = serde_json::from_str::<WSSignalMessageToNode>(&msg_node_str) {
                msg_node
            } else {
                return vec![];
            };
        match msg_node {
            WSSignalMessageToNode::Challenge(version, challenge) => {
                if version != SIGNAL_VERSION {
                    panic!(
                        "Wrong signal-server version: got {version}, expected {SIGNAL_VERSION}."
                    );
                }
                let ma = MessageAnnounce {
                    version,
                    challenge,
                    node_info: self.node_config.info.clone(),
                    signature: self.node_config.sign(challenge.to_bytes()),
                };
                vec![
                    WSSignalMessageFromNode::Announce(ma).into(),
                    WSSignalMessageFromNode::ListIDsRequest.into(),
                ]
            }
            WSSignalMessageToNode::ListIDsReply(list) => {
                vec![NetworkOut::NodeListFromWS(list)]
            }
            WSSignalMessageToNode::PeerSetup(pi) => {
                let own_id = self.node_config.info.get_id();
                let remote_node = match pi.get_remote(&own_id) {
                    Some(id) => id,
                    None => {
                        log::warn!("Got PeerSetup where this node is not involved");
                        return vec![];
                    }
                };
                concat(vec![
                    self.assure_connection(&remote_node, Direction::Incoming),
                    vec![NetworkOut::WebRTC(WebRTCConnInput::Message(
                        remote_node,
                        NCInput::Setup(pi.get_direction(&own_id), pi.message),
                    ))],
                ])
            }
            WSSignalMessageToNode::SystemConfig(fledger_config) => {
                vec![NetworkOut::SystemConfig(fledger_config)]
            }
            WSSignalMessageToNode::Error(e) => vec![NetworkOut::Error(e)],
        }
    }

    async fn msg_call(&mut self, msg: NetworkIn) -> anyhow::Result<Vec<NetworkOut>> {
        match msg {
            NetworkIn::MessageToNode(id, msg_str) => {
                log::trace!(
                    "msg_call: {}->{}: {:?} / {:?}",
                    self.node_config.info.get_id(),
                    id,
                    msg_str,
                    self.connections
                );

                Ok(concat(vec![
                    self.assure_connection(&id, Direction::Outgoing),
                    vec![NetworkOut::WebRTC(WebRTCConnInput::Message(
                        id,
                        NCInput::Text(msg_str),
                    ))],
                ]))
            }
            NetworkIn::StatsToWS(ss) => WSSignalMessageFromNode::NodeStats(ss.clone()).into(),
            NetworkIn::WSUpdateListRequest => WSSignalMessageFromNode::ListIDsRequest.into(),
            NetworkIn::Connect(id) => Ok(self.assure_connection(&id, Direction::Outgoing)),
            NetworkIn::Disconnect(id) => Ok(self.disconnect(&id).await),
            NetworkIn::Tick => {
                self.get_update -= 1;
                Ok((self.get_update == 0)
                    .then(|| {
                        self.get_update = UPDATE_INTERVAL;
                        vec![WSSignalMessageFromNode::ListIDsRequest.into()]
                    })
                    .unwrap_or(vec![]))
            }
            _ => Ok(vec![]),
        }
    }

    async fn msg_node(&mut self, id: U256, msg_nc: NCOutput) -> Vec<NetworkOut> {
        match msg_nc {
            NCOutput::Connected(_) => vec![NetworkOut::Connected(id)],
            NCOutput::Disconnected(_) => vec![NetworkOut::Disconnected(id)],
            NCOutput::Text(msg) => vec![NetworkOut::MessageFromNode(id, msg)],
            NCOutput::State(dir, state) => {
                vec![NetworkOut::ConnectionState(NetworkConnectionState {
                    id,
                    dir,
                    s: ConnStats {
                        type_local: state.type_local,
                        type_remote: state.type_remote,
                        signaling: state.signaling,
                        rx_bytes: state.rx_bytes,
                        tx_bytes: state.tx_bytes,
                        delay_ms: state.delay_ms,
                    },
                })]
            }
            NCOutput::Setup(dir, pm) => {
                let mut id_init = self.node_config.info.get_id();
                let mut id_follow = id;
                if dir == Direction::Incoming {
                    (id_init, id_follow) = (id_follow, id_init);
                }
                vec![WSSignalMessageFromNode::PeerSetup(PeerInfo {
                    id_init,
                    id_follow,
                    message: pm,
                })
                .into()]
            }
            _ => vec![],
        }
    }

    /// Connect to the given node.
    fn assure_connection(&mut self, dst: &U256, dir: Direction) -> Vec<NetworkOut> {
        if self.connections.contains(dst) {
            vec![]
        } else {
            self.connections.push(dst.clone());
            self.send_connections();
            vec![NetworkOut::WebRTC(WebRTCConnInput::Connect(*dst, dir))]
        }
    }

    /// Disconnects from a given node.
    async fn disconnect(&mut self, dst: &U256) -> Vec<NetworkOut> {
        let mut out = vec![NetworkOut::Disconnected(*dst)];
        if !self.connections.contains(dst) {
            log::trace!("Already disconnected from {}", dst);
        } else {
            self.connections.retain(|id| id != dst);
            self.send_connections();
            out.push(NetworkOut::WebRTC(WebRTCConnInput::Message(
                *dst,
                NCInput::Disconnect,
            )));
        }
        out
    }

    fn send_connections(&mut self) {
        self.tx.clone().map(|tx| {
            tx.send(self.connections.clone())
                .is_err()
                .then(|| self.tx = None)
        });
    }
}

#[platform_async_trait()]
impl SubsystemHandler<NetworkIn, NetworkOut> for Messages {
    async fn messages(&mut self, msgs: Vec<NetworkIn>) -> Vec<NetworkOut> {
        let mut out = vec![];
        for msg in msgs {
            log::trace!(
                "{}: Processing message {msg}",
                self.node_config.info.get_id()
            );
            match msg {
                NetworkIn::WebSocket(ws) => out.extend(self.msg_ws(ws).await),
                NetworkIn::WebRTC(WebRTCConnOutput::Message(id, msg)) => {
                    out.extend(self.msg_node(id, msg).await)
                }
                _ => out.extend(self.msg_call(msg).await.unwrap()),
            }
        }
        out
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
    pub fn send_msg(&mut self, dst: NodeID, msg: String) -> anyhow::Result<()> {
        self.send(NetworkIn::MessageToNode(dst, msg))
    }

    /// Requests an updated list of all connected nodes to the signalling server.
    pub fn send_list_request(&mut self) -> anyhow::Result<()> {
        self.send(NetworkIn::WSUpdateListRequest)
    }
}

impl From<WSSignalMessageFromNode> for NetworkOut {
    fn from(msg: WSSignalMessageFromNode) -> Self {
        NetworkOut::WebSocket(WSClientIn::Message(serde_json::to_string(&msg).unwrap()))
    }
}

impl From<WSSignalMessageFromNode> for anyhow::Result<Vec<NetworkOut>> {
    fn from(msg: WSSignalMessageFromNode) -> Self {
        Ok(vec![msg.into()])
    }
}

impl TranslateFrom<WSClientOut> for NetworkIn {
    fn translate(msg: WSClientOut) -> Option<Self> {
        Some(NetworkIn::WebSocket(msg))
    }
}
impl TranslateInto<WSClientIn> for NetworkOut {
    fn translate(self) -> Option<WSClientIn> {
        match self {
            NetworkOut::WebSocket(msg) => Some(msg),
            _ => None,
        }
    }
}

impl TranslateFrom<WebRTCConnOutput> for NetworkIn {
    fn translate(msg: WebRTCConnOutput) -> Option<Self> {
        Some(NetworkIn::WebRTC(msg))
    }
}

impl TranslateInto<WebRTCConnInput> for NetworkOut {
    fn translate(self) -> Option<WebRTCConnInput> {
        match self {
            NetworkOut::WebRTC(msg_webrtc) => Some(msg_webrtc),
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
            NetworkIn::Tick => write!(f, "Tick"),
            NetworkIn::WebSocket(_) => write!(f, "WebSocket"),
            NetworkIn::WebRTC(_) => write!(f, "WebRTC"),
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
            NetworkOut::WebSocket(_) => write!(f, "WebSocket"),
            NetworkOut::WebRTC(_) => write!(f, "WebRTC"),
            NetworkOut::SystemConfig(_) => write!(f, "SystemConfig"),
            NetworkOut::Error(_) => write!(f, "Error"),
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
    use flarch::start_logging;

    use super::*;

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
