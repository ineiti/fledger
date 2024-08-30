//! # Network related structures
//! 
//! The basic structure here is [`NetworkBroker`], which handles all communications
//! with the signalling server and setting up of new WebRTC connections.
//! As the [`NetworkBroker`] simply returns a [`Broker<NetworkMessage>`], a more
//! user-friendly wrapper named [`Network`] exists, which is better suited for
//! usage in a non-[`Broker`] world.
//! 
//! Both of these structures are best created with [`crate::network_start`] and 
//! [`crate::network_broker_start`].

use async_trait::async_trait;
use core::panic;
use itertools::concat;
use std::{fmt, time::Duration};
use thiserror::Error;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_stream::StreamExt;

use flarch::tasks::Interval;
use flmodules::{
    broker::{Broker, BrokerError, Subsystem, SubsystemHandler},
    nodeids::{NodeID, U256},
};

use crate::{
    config::{NodeConfig, NodeInfo},
    signal::{
        MessageAnnounce, NodeStat, WSSignalMessageFromNode, WSSignalMessageToNode, SIGNAL_VERSION,
    },
    web_rtc::{
        messages::{ConnType, PeerInfo, SetupError, SignalingState},
        node_connection::{Direction, NCInput, NCOutput},
        WebRTCConnMessage,
    },
    websocket::{WSClientInput, WSClientMessage, WSClientOutput},
};

use crate::web_rtc::node_connection::NCError;

/// This is a user-friendly version of [`NetworkBroker`].
/// Upon starting, it waits for the connection to the signalling server.
/// It has a simple API to send and receive messages to other nodes.
pub struct Network {
    broker_net: Broker<NetworkMessage>,
    tap: UnboundedReceiver<NetworkMessage>,
}

impl Network {
    /// Takes a [`NetworkBroker`] and returns the wrapped [`Network`].
    /// It waits for the [`NetworkBroker`] to be completely set up.
    /// For this reason the [`NetworkBroker`] must not have been used before.
    pub async fn start(mut broker_net: Broker<NetworkMessage>) -> Result<Self, NetworkError> {
        let (mut tap, _) = broker_net.get_tap().await?;
        let mut timeout = Interval::new_interval(Duration::from_secs(10));
        timeout.next().await;
        loop {
            tokio::select! {
                _ = timeout.next() => {
                    return Err(NetworkError::SignallingServer);
                }
                msg = tap.recv() => {
                    if matches!(msg, Some(NetworkMessage::Reply(NetReply::RcvWSUpdateList(_)))){
                        break;
                    }
                }
            }
        }
        Ok(Self { broker_net, tap })
    }

    /// Wait for a message to be received from the network.
    /// This method waits for a [`NetReply`] message to be received, which
    /// are the only messages interesting for a user.
    pub async fn recv(&mut self) -> NetReply {
        loop {
            let msg = self.tap.recv().await;
            if let Some(NetworkMessage::Reply(msg_reply)) = msg {
                return msg_reply;
            }
        }
    }

    /// Send a message to the [`NetworkBroker`] asynchronously.
    /// The message is of type [`NetCall`], as this is what the user can
    /// send to the [`NetworkBroker`].
    pub fn send(&mut self, msg: NetCall) -> Result<(), BrokerError> {
        self.broker_net.emit_msg(NetworkMessage::Call(msg))
    }

    /// Tries to send a text-message to a remote node.
    /// The [`NetworkBroker`] will start a connection with the node if there is none available.
    /// If the remote node is not available, no error is returned.
    pub fn send_msg(&mut self, dst: crate::NodeID, msg: String) -> Result<(), BrokerError> {
        self.send(NetCall::SendNodeMessage(dst, msg))
    }

    /// Requests an updated list of all connected nodes to the signalling server.
    pub fn send_list_request(&mut self) -> Result<(), BrokerError> {
        self.send(NetCall::SendWSUpdateListRequest)
    }
}

#[derive(Error, Debug)]
/// Possible errors from the [`NetworkBroker`].
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

/// The [`NetworkBroker`] handles setting up webRTC connections for all connected nodes.
/// It can handle incoming and outgoing connections.
/// 
/// In this version, all connection setup (signalling) is going through a websocket.
/// In a next version, it should also be possible to setup new connections (signalling) through existing WebRTC
/// connections: If A and B are connected, and B and C are connected, C can connect to A by using
/// B as a signalling server.
pub struct NetworkBroker {
    node_config: NodeConfig,
    get_update: usize,
    connections: Vec<NodeID>,
}

const UPDATE_INTERVAL: usize = 10;

impl NetworkBroker {
    /// Starts a new [`NetworkBroker`] and returns a [`Broker<NetworkMessage>`] which can be linked
    /// to other brokers.
    /// If you just need a simple send/receive interface, use the [`Network`].
    pub async fn start(
        node_config: NodeConfig,
        ws: Broker<WSClientMessage>,
        web_rtc: Broker<WebRTCConnMessage>,
    ) -> Result<Broker<NetworkMessage>, NetworkError> {
        let mut broker = Broker::new();
        broker
            .add_subsystem(Subsystem::Handler(Box::new(Self {
                node_config,
                get_update: UPDATE_INTERVAL,
                connections: vec![],
            })))
            .await?;
        broker
            .link_bi(ws, Box::new(Self::from_ws), Box::new(Self::to_ws))
            .await?;
        broker
            .link_bi(
                web_rtc,
                Box::new(Self::from_web_rtc),
                Box::new(Self::to_web_rtc),
            )
            .await?;
        Ok(broker)
    }

    /// Processes incoming messages from the signalling server.
    /// This can be either messages requested by this node, or connection
    /// setup requests from another node.
    async fn msg_ws(&mut self, msg: WSClientOutput) -> Vec<NetworkMessage> {
        let msg_node_str = match msg {
            WSClientOutput::Message(msg) => msg,
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
                vec![NetReply::RcvWSUpdateList(list).into()]
            }
            WSSignalMessageToNode::PeerSetup(pi) => {
                let own_id = self.node_config.info.get_id();
                let remote_node = match pi.get_remote(&own_id) {
                    Some(id) => id,
                    None => {
                        log::warn!("Got PeerSetup from unknown node");
                        return vec![];
                    }
                };
                concat(vec![
                    if !self.connections.contains(&remote_node) {
                        self.connect(&remote_node)
                    } else {
                        vec![]
                    },
                    vec![
                        NCInput::Setup((pi.get_direction(&own_id), pi.message)).to_net(remote_node)
                    ],
                ])
            }
        }
    }

    async fn msg_call(&mut self, msg: NetCall) -> Result<Vec<NetworkMessage>, NetworkError> {
        match msg {
            NetCall::SendNodeMessage(id, msg_str) => {
                log::trace!(
                    "msg_call: {}->{}: {:?} / {:?}",
                    self.node_config.info.get_id(),
                    id,
                    msg_str,
                    self.connections
                );

                Ok(concat(vec![
                    if !self.connections.contains(&id) {
                        log::warn!(
                            "Got message to unconnected node and connecting first to {}",
                            id
                        );
                        self.connect(&id)
                    } else {
                        vec![]
                    },
                    vec![NCInput::Text(msg_str).to_net(id)],
                ]))
            }
            NetCall::SendWSStats(ss) => Ok(WSSignalMessageFromNode::NodeStats(ss.clone()).into()),
            NetCall::SendWSUpdateListRequest => Ok(WSSignalMessageFromNode::ListIDsRequest.into()),
            NetCall::Connect(id) => Ok(self.connect(&id)),
            NetCall::Disconnect(id) => Ok(self.disconnect(&id).await),
            NetCall::Tick => {
                self.get_update -= 1;
                Ok((self.get_update == 0)
                    .then(|| {
                        self.get_update = UPDATE_INTERVAL;
                        vec![WSSignalMessageFromNode::ListIDsRequest.into()]
                    })
                    .unwrap_or(vec![]))
            }
        }
    }

    async fn msg_node(&mut self, id: U256, msg_nc: NCOutput) -> Vec<NetworkMessage> {
        match msg_nc {
            NCOutput::Connected(_) => vec![NetReply::Connected(id).into()],
            NCOutput::Disconnected(_) => vec![NetReply::Disconnected(id).into()],
            NCOutput::Text(msg) => vec![NetReply::RcvNodeMessage(id, msg).into()],
            NCOutput::State((dir, state)) => {
                vec![NetReply::ConnectionState(NetworkConnectionState {
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
            NCOutput::Setup((dir, pm)) => {
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
        }
    }

    /// Connect to the given node.
    fn connect(&mut self, dst: &U256) -> Vec<NetworkMessage> {
        let mut out = vec![NetReply::Connected(*dst).into()];
        if self.connections.contains(dst) {
            log::warn!("Already connected to {}", dst);
        } else {
            self.connections.push(dst.clone());
            out.push(NetworkMessage::WebRTC(WebRTCConnMessage::Connect(*dst)));
        }
        out
    }

    /// Disconnects from a given node.
    async fn disconnect(&mut self, dst: &U256) -> Vec<NetworkMessage> {
        let mut out = vec![NetReply::Disconnected(*dst).into()];
        if !self.connections.contains(dst) {
            log::warn!("Already disconnected from {}", dst);
        } else {
            self.connections.retain(|id| id != dst);
            out.push(NCInput::Disconnect.to_net(*dst));
        }
        out
    }

    // Translator functions

    fn to_ws(msg: NetworkMessage) -> Option<WSClientMessage> {
        match msg {
            NetworkMessage::WebSocket(msg) => matches!(msg, WSClientMessage::Input(_)).then(|| msg),
            _ => None,
        }
    }

    fn from_ws(msg: WSClientMessage) -> Option<NetworkMessage> {
        matches!(msg, WSClientMessage::Output(_)).then(|| NetworkMessage::WebSocket(msg))
    }

    fn to_web_rtc(msg: NetworkMessage) -> Option<WebRTCConnMessage> {
        if let NetworkMessage::WebRTC(msg_webrtc) = msg {
            match msg_webrtc {
                WebRTCConnMessage::InputNC(_) | WebRTCConnMessage::Connect(_) => Some(msg_webrtc),
                WebRTCConnMessage::OutputNC(_) => None,
            }
        } else {
            None
        }
    }

    fn from_web_rtc(msg: WebRTCConnMessage) -> Option<NetworkMessage> {
        matches!(msg, WebRTCConnMessage::OutputNC(_)).then(|| NetworkMessage::WebRTC(msg))
    }
}

#[cfg_attr(feature = "nosend", async_trait(?Send))]
#[cfg_attr(not(feature = "nosend"), async_trait)]
impl SubsystemHandler<NetworkMessage> for NetworkBroker {
    async fn messages(&mut self, bms: Vec<NetworkMessage>) -> Vec<NetworkMessage> {
        let mut out = vec![];
        for msg in bms {
            log::trace!(
                "{}: Processing message {msg}",
                self.node_config.info.get_id()
            );
            match msg {
                NetworkMessage::Call(c) => out.extend(self.msg_call(c).await.unwrap()),
                NetworkMessage::WebSocket(WSClientMessage::Output(ws)) => {
                    out.extend(self.msg_ws(ws).await)
                }
                NetworkMessage::WebRTC(WebRTCConnMessage::OutputNC((id, msg))) => {
                    out.extend(self.msg_node(id, msg).await)
                }
                _ => {}
            }
        }
        out
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
/// A Message to/from the [`NetworkBroker`].
/// The [`NetworkMessage::Call`] and [`NetworkMessage::Reply`] messages are directly related to the [`NetworkBroker`], while
/// the [`NetworkMessage::WebSocket`] and [`NetworkMessage::WebRTC`] messages are used to link to those modules.
pub enum NetworkMessage {
    /// A message to the [`NetworkBroker`]
    Call(NetCall),
    /// A return message from the [`NetworkBroker`]
    Reply(NetReply),
    /// Messages to/from the WebSocket.
    WebSocket(WSClientMessage),
    /// Messages to/from the WebRTC subsystem.
    WebRTC(WebRTCConnMessage),
}

impl fmt::Display for NetworkMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NetworkMessage::Call(c) => write!(f, "Call({})", c),
            NetworkMessage::Reply(r) => write!(f, "Reply({})", r),
            NetworkMessage::WebSocket(_) => write!(f, "WebSocket()"),
            NetworkMessage::WebRTC(_) => write!(f, "WebRTC()"),
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
/// These are similar to public methods on a structure.
/// Sending these messages will call the linked actions.
pub enum NetCall {
    /// Sends a new text message to the node.
    /// The [`NetworkBroker`] will try to set up a connection with the remote node,
    /// if no such connection exists yet.
    /// If the node is not connected to the signalling handler, nothing happens.
    SendNodeMessage(NodeID, String),
    /// Sends some stats to the signalling server to monitor the overall health of
    /// the system.
    SendWSStats(Vec<NodeStat>),
    /// Requests a new list of currenlty connected nodes to the signalling server.
    SendWSUpdateListRequest,
    /// Connect to the given node.
    /// If the node is not connected to the signalling server, no connection is made,
    /// and no error is produced.
    Connect(NodeID),
    /// Manually disconnect from the given node.
    /// If there is no connection to this node, no error is produced.
    Disconnect(NodeID),
    /// This message should be sent once a second to allow calculations of timeouts.
    Tick,
}

impl fmt::Display for NetCall {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NetCall::SendNodeMessage(_, _) => write!(f, "SendNodeMessage()"),
            NetCall::SendWSStats(_) => write!(f, "SendWSStats()"),
            NetCall::SendWSUpdateListRequest => write!(f, "SendWSUpdateListRequest"),
            NetCall::Connect(_) => write!(f, "Connect()"),
            NetCall::Disconnect(_) => write!(f, "Disconnect()"),
            NetCall::Tick => write!(f, "Tick"),
        }
    }
}

impl From<NetCall> for NetworkMessage {
    fn from(msg: NetCall) -> Self {
        Self::Call(msg)
    }
}

impl From<WSSignalMessageFromNode> for Vec<NetworkMessage> {
    fn from(msg: WSSignalMessageFromNode) -> Self {
        vec![NetworkMessage::WebSocket(
            WSClientInput::Message(serde_json::to_string(&msg).unwrap()).into(),
        )]
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
/// Messages sent from the [`NetworkBroker`] to the user.
pub enum NetReply {
    /// A new message has been received from the given node.
    RcvNodeMessage(NodeID, String),
    /// An updated list coming from the signalling server.
    RcvWSUpdateList(Vec<NodeInfo>),
    /// Whenever the state of a connection changes, this message is 
    /// sent to the user.
    ConnectionState(NetworkConnectionState),
    /// A node has been successfully connected.
    Connected(NodeID),
    /// A node has been disconnected.
    Disconnected(NodeID),
}

impl fmt::Display for NetReply {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NetReply::RcvNodeMessage(_, _) => write!(f, "RcvNodeMessage()"),
            NetReply::RcvWSUpdateList(_) => write!(f, "RcvWSUpdateList()"),
            NetReply::ConnectionState(_) => write!(f, "ConnectionState()"),
            NetReply::Connected(_) => write!(f, "Connected()"),
            NetReply::Disconnected(_) => write!(f, "Disconnected()"),
        }
    }
}

impl From<NetReply> for NetworkMessage {
    fn from(msg: NetReply) -> Self {
        Self::Reply(msg)
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

impl From<NetworkConnectionState> for NetworkMessage {
    fn from(msg: NetworkConnectionState) -> Self {
        NetReply::ConnectionState(msg).into()
    }
}

impl From<WSSignalMessageFromNode> for NetworkMessage {
    fn from(msg: WSSignalMessageFromNode) -> Self {
        Into::<WSClientMessage>::into(WSClientInput::Message(serde_json::to_string(&msg).unwrap()))
            .into()
    }
}

impl From<WSClientMessage> for NetworkMessage {
    fn from(msg: WSClientMessage) -> Self {
        NetworkMessage::WebSocket(msg)
    }
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
    fn test_serialize() -> Result<(), Box<dyn std::error::Error>> {
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
