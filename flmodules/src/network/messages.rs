use core::panic;
use itertools::concat;
use std::fmt::Display;
use tokio::sync::watch;

use flarch::{
    broker::SubsystemHandler,
    nodeids::{NodeID, U256},
    platform_async_trait,
    web_rtc::{
        messages::PeerInfo,
        node_connection::{Direction, NCInput, NCOutput},
        websocket::{WSClientIn, WSClientOut},
        WebRTCConnInput, WebRTCConnOutput,
    },
};

use crate::{
    network::{
        broker::{ConnStats, NetworkConnectionState},
        signal::{MessageAnnounce, WSSignalMessageFromNode, WSSignalMessageToNode, SIGNAL_VERSION},
    },
    nodeconfig::NodeConfig,
};

use super::broker::{NetworkIn, NetworkOut};

#[derive(Debug, Clone, PartialEq)]
pub(super) enum InternIn {
    Network(NetworkIn),
    /// This message should be sent once a second to allow calculations of timeouts.
    Tick,
    WebSocket(WSClientOut),
    WebRTC(WebRTCConnOutput),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum InternOut {
    Network(NetworkOut),
    /// This is used in the From and Into methods
    WebSocket(WSClientIn),
    WebRTC(WebRTCConnInput),
}

const UPDATE_INTERVAL: usize = 10;

pub(super) struct Messages {
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
    async fn msg_ws(&mut self, msg: WSClientOut) -> Vec<InternOut> {
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
                    vec![InternOut::WebRTC(WebRTCConnInput::Message(
                        remote_node,
                        NCInput::Setup(pi.get_direction(&own_id), pi.message),
                    ))],
                ])
            }
            WSSignalMessageToNode::SystemConfig(fledger_config) => {
                vec![NetworkOut::SystemConfig(fledger_config)].into_vec()
            }
        }
    }

    async fn msg_call(&mut self, msg: NetworkIn) -> anyhow::Result<Vec<InternOut>> {
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
                    if !self.connections.contains(&id) {
                        self.connect(&id)
                    } else {
                        vec![]
                    },
                    vec![InternOut::WebRTC(WebRTCConnInput::Message(
                        id,
                        NCInput::Text(msg_str),
                    ))],
                ]))
            }
            NetworkIn::StatsToWS(ss) => WSSignalMessageFromNode::NodeStats(ss.clone()).into(),
            NetworkIn::WSUpdateListRequest => WSSignalMessageFromNode::ListIDsRequest.into(),
            NetworkIn::Connect(id) => Ok(self.connect(&id)),
            NetworkIn::Disconnect(id) => Ok(self.disconnect(&id).await),
        }
    }

    async fn msg_tick(&mut self) -> Vec<InternOut> {
        self.get_update -= 1;
        (self.get_update == 0)
            .then(|| {
                self.get_update = UPDATE_INTERVAL;
                vec![WSSignalMessageFromNode::ListIDsRequest.into()]
            })
            .unwrap_or(vec![])
    }

    async fn msg_node(&mut self, id: U256, msg_nc: NCOutput) -> Vec<InternOut> {
        match msg_nc {
            NCOutput::Connected(_) => vec![NetworkOut::Connected(id).into()],
            NCOutput::Disconnected(_) => vec![NetworkOut::Disconnected(id).into()],
            NCOutput::Text(msg) => vec![NetworkOut::MessageFromNode(id, msg).into()],
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
    fn connect(&mut self, dst: &U256) -> Vec<InternOut> {
        if self.connections.contains(dst) {
            log::trace!("Already connected to {}", dst);
            vec![]
        } else {
            self.connections.push(dst.clone());
            self.send_connections();
            vec![InternOut::WebRTC(WebRTCConnInput::Connect(*dst))]
        }
    }

    /// Disconnects from a given node.
    async fn disconnect(&mut self, dst: &U256) -> Vec<InternOut> {
        let mut out = vec![NetworkOut::Disconnected(*dst).into()];
        if !self.connections.contains(dst) {
            log::trace!("Already disconnected from {}", dst);
        } else {
            self.connections.retain(|id| id != dst);
            self.send_connections();
            out.push(InternOut::WebRTC(WebRTCConnInput::Message(
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
        let mut out = vec![];
        for msg in msgs {
            log::trace!(
                "{}: Processing message {msg}",
                self.node_config.info.get_id()
            );
            match msg {
                InternIn::WebSocket(ws) => out.extend(self.msg_ws(ws).await),
                InternIn::WebRTC(WebRTCConnOutput::Message(id, msg)) => {
                    out.extend(self.msg_node(id, msg).await)
                }
                InternIn::Network(net) => out.extend(self.msg_call(net).await.unwrap()),
                InternIn::Tick => out.extend(self.msg_tick().await),
            }
        }
        out
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

impl From<WebRTCConnInput> for InternOut {
    fn from(value: WebRTCConnInput) -> Self {
        InternOut::WebRTC(value)
    }
}

impl From<WSSignalMessageFromNode> for InternOut {
    fn from(msg: WSSignalMessageFromNode) -> Self {
        InternOut::WebSocket(WSClientIn::Message(serde_json::to_string(&msg).unwrap()))
    }
}

impl From<WSSignalMessageFromNode> for anyhow::Result<Vec<InternOut>> {
    fn from(msg: WSSignalMessageFromNode) -> Self {
        Ok(vec![msg.into()])
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
