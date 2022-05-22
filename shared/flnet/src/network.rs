use crate::signal::websocket::WSClientInput;
use itertools::concat;
use std::fmt;

use async_trait::async_trait;
use thiserror::Error;

use flmodules::{
    broker::{Broker, BrokerError, Destination, Subsystem, SubsystemListener, Translate},
    nodeids::U256,
};

use crate::{
    config::{NodeConfig, NodeInfo},
    network::node_connection::NCInput,
    signal::{
        web_rtc::{
            ConnType, MessageAnnounce, NodeStat, PeerInfo, SetupError, SignalingState,
            WSSignalMessageFromNode, WSSignalMessageToNode, WebRTCSpawner,
        },
        websocket::{WSClientMessage, WSClientOutput},
    },
};

pub mod node_connection;
use node_connection::{NCError, NodeConnection};

use self::node_connection::{Direction, NCMessage, NCOutput};

/**
 * The Network structure handles setting up webRTC connections for all connected nodes.
 * It can handle incoming and outgoing connections.
 * In this version, all connection setup (signalling) is going through a websocket.
 * In a next version, it should also be possible to setup new connections (signalling) through existing WebRTC
 * connections: If A and B are connected, and B and C are connected, C can connect to A by using
 * B as a signalling server.
 */

#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Connection not found")]
    ConnectionMissing,
    #[error("Cannot connect to myself")]
    ConnectMyself,
    #[error(transparent)]
    SerdeJSON(#[from] serde_json::Error),
    #[error(transparent)]
    NodeConnection(#[from] NCError),
    #[error(transparent)]
    Broker(#[from] BrokerError),
    #[error(transparent)]
    Setup(#[from] SetupError),
}

pub struct Network {
    broker: Broker<NetworkMessage>,
    web_rtc: WebRTCSpawner,
    node_config: NodeConfig,
    connections: Vec<U256>,
    get_update: usize,
}

const UPDATE_INTERVAL: usize = 10;

impl Network {
    pub async fn start(
        node_config: NodeConfig,
        ws: Broker<WSClientMessage>,
        web_rtc: WebRTCSpawner,
    ) -> Result<Broker<NetworkMessage>, NetworkError> {
        let mut broker = Broker::new();
        broker
            .add_subsystem(Subsystem::Handler(Box::new(Self {
                broker: broker.clone(),
                web_rtc,
                node_config,
                connections: vec![],
                get_update: UPDATE_INTERVAL,
            })))
            .await?;
        broker
            .link_bi(ws, Box::new(Self::from_ws), Box::new(Self::to_ws))
            .await;
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
                let ma = MessageAnnounce {
                    version,
                    challenge,
                    node_info: self.node_config.our_node.clone(),
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
                let own_id = self.node_config.our_node.get_id();
                let remote_node = match pi.get_remote(&own_id) {
                    Some(id) => id,
                    None => {
                        log::warn!("Got PeerSetup from unknown node");
                        return vec![];
                    }
                };
                self.send(
                    &remote_node,
                    NCInput::Setup((pi.get_direction(&own_id), pi.message)),
                )
                .await
                .unwrap_or_else(|err| {
                    log::error!("Couldn't send: {err:?}");
                    vec![]
                })
            }
        }
    }

    async fn msg_call(&mut self, msg: NetCall) -> Result<Vec<NetworkMessage>, NetworkError> {
        match msg {
            NetCall::SendNodeMessage((id, msg_str)) => {
                log::trace!(
                    "msg_call: {}->{}: {:?}",
                    self.node_config.our_node.get_id(),
                    id,
                    msg_str
                );
                self.send_msg(&id, msg_str).await
            }
            NetCall::SendWSStats(ss) => Ok(WSSignalMessageFromNode::NodeStats(ss.clone()).into()),
            NetCall::SendWSUpdateListRequest => Ok(WSSignalMessageFromNode::ListIDsRequest.into()),
            NetCall::SendWSClearNodes => Ok(WSSignalMessageFromNode::ClearNodes.into()),
            NetCall::SendWSPeer(pi) => Ok(WSSignalMessageFromNode::PeerSetup(pi).into()),
            NetCall::Connect(id) => return Ok(vec![self.connect(&id).await?]),
            NetCall::Disconnect(id) => return Ok(self.disconnect(&id).await?),
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
            NCOutput::Text(msg) => vec![NetReply::RcvNodeMessage((id, msg)).into()],
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
                let mut id_init = self.node_config.our_node.get_id();
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

    /// Sends a message to the node dst.
    /// If no connection is active yet, a new one will be created.
    async fn send(
        &mut self,
        dst: &U256,
        msg: NCInput,
    ) -> Result<Vec<NetworkMessage>, NetworkError> {
        self.ensure_connection(dst).await?;
        Ok(vec![NetworkMessage::NodeConnection((
            *dst,
            NCMessage::Input(msg),
        ))])
    }

    // Converts a message to an NCInput and makes sure that the connection exists.
    async fn send_msg(
        &mut self,
        dst: &U256,
        msg: String,
    ) -> Result<Vec<NetworkMessage>, NetworkError> {
        self.send(dst, NCInput::Text(msg)).await
    }

    /// Connect to the given node.
    async fn connect(&mut self, dst: &U256) -> Result<NetworkMessage, NetworkError> {
        self.ensure_connection(dst).await?;
        return Ok(NetReply::Connected(*dst).into());
    }

    /// Disconnects from a given node.
    async fn disconnect(&mut self, dst: &U256) -> Result<Vec<NetworkMessage>, NetworkError> {
        let msgs = self.send(dst, NCInput::Disconnect).await?;
        return Ok(concat([msgs, vec![NetReply::Disconnected(*dst).into()]]));
    }

    /// Ensures that a given connection exists.
    async fn ensure_connection(&mut self, id: &U256) -> Result<(), NetworkError> {
        if *id == self.node_config.our_node.get_id() {
            return Err(NetworkError::ConnectMyself);
        }

        if !self.connections.contains(id) {
            let nc = NodeConnection::new(&self.web_rtc).await?;
            self.broker
                .link_bi(
                    nc.clone(),
                    Self::from_nc(id.clone()),
                    Self::to_nc(id.clone()),
                )
                .await;
            self.connections.push(*id);
        }

        Ok(())
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

    fn from_nc(id: U256) -> Translate<NCMessage, NetworkMessage> {
        Box::new(move |msg| {
            matches!(msg, NCMessage::Output(_)).then(|| NetworkMessage::NodeConnection((id, msg)))
        })
    }

    fn to_nc(id: U256) -> Translate<NetworkMessage, NCMessage> {
        Box::new(move |msg| {
            if let NetworkMessage::NodeConnection((dst, msg_nc)) = msg {
                if dst == id && matches!(msg_nc, NCMessage::Input(_)) {
                    return Some(msg_nc);
                }
            }
            None
        })
    }
}

#[cfg_attr(feature = "nosend", async_trait(?Send))]
#[cfg_attr(not(feature = "nosend"), async_trait)]
impl SubsystemListener<NetworkMessage> for Network {
    async fn messages(&mut self, bms: Vec<NetworkMessage>) -> Vec<(Destination, NetworkMessage)> {
        let mut out = vec![];
        for msg in bms {
            log::trace!(
                "{}: Processing message {msg}",
                self.node_config.our_node.get_id()
            );
            match msg {
                NetworkMessage::Call(c) => out.extend(self.msg_call(c).await.unwrap()),
                NetworkMessage::WebSocket(WSClientMessage::Output(ws)) => {
                    out.extend(self.msg_ws(ws).await)
                }
                NetworkMessage::NodeConnection((id, NCMessage::Output(msg))) => {
                    out.extend(self.msg_node(id, msg).await)
                }
                _ => {}
            }
        }
        out.into_iter().map(|m| (Destination::Others, m)).collect()
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum NetworkMessage {
    Call(NetCall),
    Reply(NetReply),
    WebSocket(WSClientMessage),
    NodeConnection((U256, NCMessage)),
}

impl fmt::Display for NetworkMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NetworkMessage::Call(c) => write!(f, "Call({})", c),
            NetworkMessage::Reply(r) => write!(f, "Reply({})", r),
            NetworkMessage::WebSocket(_) => write!(f, "WebSocket()"),
            NetworkMessage::NodeConnection((id, _)) => write!(f, "NodeConnection({}, ..)", id),
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum NetCall {
    SendNodeMessage((U256, String)),
    SendWSStats(Vec<NodeStat>),
    SendWSClearNodes,
    SendWSUpdateListRequest,
    SendWSPeer(PeerInfo),
    Connect(U256),
    Disconnect(U256),
    Tick,
}

impl fmt::Display for NetCall {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NetCall::SendNodeMessage(_) => write!(f, "SendNodeMessage()"),
            NetCall::SendWSStats(_) => write!(f, "SendWSStats()"),
            NetCall::SendWSClearNodes => write!(f, "SendWSClearNodes"),
            NetCall::SendWSUpdateListRequest => write!(f, "SendWSUpdateListRequest"),
            NetCall::SendWSPeer(_) => write!(f, "SendWSPeer()"),
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
            WSClientOutput::Message(serde_json::to_string(&msg).unwrap()).into(),
        )]
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum NetReply {
    RcvNodeMessage((U256, String)),
    RcvWSUpdateList(Vec<NodeInfo>),
    ConnectionState(NetworkConnectionState),
    Connected(U256),
    Disconnected(U256),
}

impl fmt::Display for NetReply {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NetReply::RcvNodeMessage(_) => write!(f, "RcvNodeMessage()"),
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
pub struct NetworkConnectionState {
    pub id: U256,
    pub dir: Direction,
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
pub struct ConnStats {
    pub type_local: ConnType,
    pub type_remote: ConnType,
    pub signaling: SignalingState,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
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
