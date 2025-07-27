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
use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Display},
    time::Duration,
};
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
        websocket::{BrokerWSClient, WSClientIn, WSClientOut, WSServerIn, WSServerOut},
        BrokerWebRTCConn, WebRTCConnIn, WebRTCConnOut,
    },
};

use crate::{
    network::{
        node_signal::NodeSignal,
        signal::{
            MessageAnnounce, NodeStat, WSSignalMessageFromNode, WSSignalMessageToNode,
            SIGNAL_VERSION,
        },
    },
    nodeconfig::{NodeConfig, NodeInfo},
    router::messages::NetworkWrapper,
    timer::Timer,
};

use super::signal::FledgerConfig;

pub type BrokerNetwork = Broker<NetworkIn, NetworkOut>;

pub const MODULE_NAME: &str = "Network";

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
        let (messages, connections) = Messages::start(node_config).await?;
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

#[derive(Debug, Serialize, Deserialize)]
pub(super) enum ModuleMessage {
    String(String),
    SignalIn(WSClientOut),
    SignalOut(WSClientIn),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum InternIn {
    Network(NetworkIn),
    Signal(WSServerIn),
    Tick,
    WebSocket(WSClientOut),
    WebRTC(WebRTCConnOut),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum InternOut {
    Network(NetworkOut),
    Signal(WSServerOut),
    WebSocket(WSClientIn),
    WebRTC(WebRTCConnIn),
}

const UPDATE_INTERVAL: usize = 10;

pub(super) struct Messages {
    node_config: NodeConfig,
    get_update: usize,
    connections: Vec<NodeID>,
    tx: Option<watch::Sender<Vec<NodeID>>>,
    node_signal: NodeSignal,
}

impl Messages {
    pub async fn start(
        node_config: NodeConfig,
    ) -> anyhow::Result<(Self, watch::Receiver<Vec<NodeID>>)> {
        let (tx, rx) = watch::channel(vec![]);
        Ok((
            Self {
                node_config,
                get_update: UPDATE_INTERVAL,
                connections: vec![],
                tx: Some(tx),
                node_signal: NodeSignal::start().await?,
            },
            rx,
        ))
    }

    /// Processes incoming messages from the signalling server.
    /// This can be either messages requested by this node, or connection
    /// setup requests from another node.
    fn msg_ws(&mut self, msg: WSClientOut) -> Vec<InternOut> {
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
                    WSSignalMessageFromNode::Announce(ma),
                    WSSignalMessageFromNode::ListIDsRequest,
                ]
                .into_vec()
            }
            WSSignalMessageToNode::ListIDsReply(list) => {
                vec![NetworkOut::NodeListFromWS(list).into()]
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
                let mut out = vec![];
                let dir = pi.get_direction(&own_id);
                if !self.connections.contains(&remote_node) {
                    out.append(&mut self.connect(&remote_node, dir.clone()));
                }
                out.push(InternOut::WebRTC(WebRTCConnIn::Message(
                    remote_node,
                    NCInput::Setup(dir, pi.message),
                )));
                out
            }
            WSSignalMessageToNode::SystemConfig(fledger_config) => {
                vec![NetworkOut::SystemConfig(fledger_config)].into_vec()
            }
        }
    }

    fn msg_net(&mut self, msg: NetworkIn) -> Vec<InternOut> {
        match msg {
            NetworkIn::MessageToNode(id, msg_nw) => {
                log::trace!(
                    "msg_call: {}->{}: {:?} / {:?}",
                    self.node_config.info.get_id(),
                    id,
                    msg_nw,
                    self.connections
                );

                let mut out = vec![];
                if !self.connections.contains(&id) {
                    out.append(&mut self.connect(&id, Direction::Outgoing));
                }
                if let Ok(s) = serde_yaml::to_string(&msg_nw) {
                    out.push(InternOut::WebRTC(WebRTCConnIn::Message(
                        id,
                        NCInput::Text(s),
                    )));
                }
                out
            }
            NetworkIn::StatsToWS(ss) => WSSignalMessageFromNode::NodeStats(ss.clone()).into(),
            NetworkIn::WSUpdateListRequest => WSSignalMessageFromNode::ListIDsRequest.into(),
            NetworkIn::Connect(id) => self.connect(&id, Direction::Outgoing),
            NetworkIn::Disconnect(id) => self.disconnect(&id),
        }
    }

    fn msg_tick(&mut self) -> Vec<InternOut> {
        self.get_update -= 1;
        (self.get_update == 0)
            .then(|| {
                self.get_update = UPDATE_INTERVAL;
                vec![WSSignalMessageFromNode::ListIDsRequest.into()]
            })
            .unwrap_or(vec![])
    }

    fn msg_wrapper(&mut self, id: NodeID, msg: String) -> Vec<InternOut> {
        match serde_yaml::from_str::<NetworkWrapper>(&msg) {
            Ok(nw) => {
                if let Some(net_msg) = nw.unwrap_yaml::<ModuleMessage>(MODULE_NAME) {
                    match net_msg {
                        ModuleMessage::String(_) => {}
                        ModuleMessage::SignalIn(_) => todo!(),
                        ModuleMessage::SignalOut(_) => todo!(),
                    }
                }
                return vec![NetworkOut::MessageFromNode(id, nw).into()];
            }
            Err(e) => log::debug!("Couldn't unwrap {msg}: {e:?}"),
        }
        vec![]
    }

    fn msg_rtc(&mut self, id: NodeID, msg_nc: NCOutput) -> Vec<InternOut> {
        match msg_nc {
            NCOutput::Connected(_) => vec![NetworkOut::Connected(id).into()],
            NCOutput::Disconnected(_) => vec![NetworkOut::Disconnected(id).into()],
            NCOutput::Text(msg) => self.msg_wrapper(id, msg),
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
                })
                .into()]
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
    /// TODO: differentiate connection_setup and connected
    fn connect(&mut self, dst: &U256, dir: Direction) -> Vec<InternOut> {
        if self.connections.contains(dst) {
            vec![]
        } else {
            self.connections.push(dst.clone());
            self.send_connections();
            vec![InternOut::WebRTC(WebRTCConnIn::Connect(*dst, dir))]
        }
    }

    /// Disconnects from a given node.
    fn disconnect(&mut self, dst: &U256) -> Vec<InternOut> {
        let mut out = vec![NetworkOut::Disconnected(*dst).into()];
        if !self.connections.contains(dst) {
            log::trace!("Already disconnected from {}", dst);
        } else {
            self.connections.retain(|id| id != dst);
            self.send_connections();
            out.push(InternOut::WebRTC(WebRTCConnIn::Message(
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
impl SubsystemHandler<InternIn, InternOut> for Messages {
    async fn messages(&mut self, msgs: Vec<InternIn>) -> Vec<InternOut> {
        let id = self.node_config.info.get_id();
        msgs.into_iter()
            .inspect(|msg| log::trace!("{id}: Processing message {msg}",))
            .flat_map(|msg| match msg {
                InternIn::WebSocket(ws) => __self.msg_ws(ws),
                InternIn::WebRTC(WebRTCConnOut::Message(id, msg)) => __self.msg_rtc(id, msg),
                InternIn::Network(net) => __self.msg_net(net),
                InternIn::Tick => __self.msg_tick(),
                InternIn::Signal(wsserver_in) => todo!(),
            })
            .collect::<Vec<_>>()
    }
}

impl From<NetworkOut> for InternOut {
    fn from(value: NetworkOut) -> Self {
        InternOut::Network(value)
    }
}

impl From<WSClientIn> for InternOut {
    fn from(value: WSClientIn) -> Self {
        InternOut::WebSocket(value)
    }
}

impl From<WebRTCConnIn> for InternOut {
    fn from(value: WebRTCConnIn) -> Self {
        InternOut::WebRTC(value)
    }
}

impl From<WSSignalMessageFromNode> for InternOut {
    fn from(msg: WSSignalMessageFromNode) -> Self {
        InternOut::WebSocket(WSClientIn::Message(serde_json::to_string(&msg).unwrap()))
    }
}

impl From<WSSignalMessageFromNode> for Vec<InternOut> {
    fn from(msg: WSSignalMessageFromNode) -> Self {
        vec![msg.into()]
    }
}

impl Display for InternIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InternIn::Network(network_in) => {
                write!(f, "InternalIn::Network({})", network_in)
            }
            InternIn::Tick => write!(f, "InternalIn::Tick"),
            InternIn::WebSocket(wsclient_out) => {
                write!(f, "InternalIn::WebSocket({:?})", wsclient_out)
            }
            InternIn::WebRTC(web_rtcconn_output) => {
                write!(f, "InternalIn::WebRTC({:?})", web_rtcconn_output)
            }
            InternIn::Signal(wsserver_in) => write!(f, "InternalIn::Signal({:?})", wsserver_in),
        }
    }
}

pub trait IntoVec<D> {
    fn into_vec(self) -> Vec<D>;
}

impl<E, D> IntoVec<D> for Vec<E>
where
    D: From<E>,
{
    fn into_vec(self) -> Vec<D> {
        self.into_iter().map(std::convert::Into::into).collect()
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
            NetworkWrapper::wrap_yaml(MODULE_NAME, &ModuleMessage::String(msg))?,
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

impl TranslateFrom<WebRTCConnOut> for InternIn {
    fn translate(msg: WebRTCConnOut) -> Option<Self> {
        Some(InternIn::WebRTC(msg))
    }
}

impl TranslateInto<WebRTCConnIn> for InternOut {
    fn translate(self) -> Option<WebRTCConnIn> {
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
