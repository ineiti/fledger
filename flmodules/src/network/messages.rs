//! Handles all the incoming messages and produces messages for the modules
//! connected to [Network].

use core::panic;
use serde::{Deserialize, Serialize};
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
        WebRTCConnIn, WebRTCConnOut,
    },
};

use crate::{
    network::{
        broker::{ConnStats, NetworkConnectionState, NetworkIn, NetworkOut, MODULE_NAME},
        signal::{MessageAnnounce, WSSignalMessageFromNode, WSSignalMessageToNode, SIGNAL_VERSION},
    },
    nodeconfig::NodeConfig,
    router::messages::NetworkWrapper,
};

#[derive(Debug, Serialize, Deserialize)]
pub(super) enum ModuleMessage {
    SignalIn(WSClientOut),
    SignalOut(WSClientIn),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum InternIn {
    Network(NetworkIn),
    WebSocket(WSClientOut),
    WebRTC(WebRTCConnOut),
    Tick,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum InternOut {
    Network(NetworkOut),
    WebSocket(WSClientIn),
    WebRTC(WebRTCConnIn),
}

const UPDATE_INTERVAL: usize = 10;

pub(super) struct Messages {
    node_config: NodeConfig,
    get_update: usize,
    connections: Vec<NodeID>,
    tx: Option<watch::Sender<Vec<NodeID>>>,
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
            WSSignalMessageToNode::Error(e) => vec![InternOut::Network(NetworkOut::Error(e))],
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
                InternIn::WebSocket(ws) => self.msg_ws(ws),
                InternIn::WebRTC(WebRTCConnOut::Message(id, msg)) => self.msg_rtc(id, msg),
                InternIn::Network(net) => self.msg_net(net),
                InternIn::Tick => self.msg_tick(),
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
