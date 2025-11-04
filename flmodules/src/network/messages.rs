//! Handles all the incoming messages and produces messages for the modules
//! connected to [Network].

use core::panic;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt::Display};
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
        local_signal::{LocalSignalIn, LocalSignalOut},
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
    LocalSignal(LocalSignalOut),
    WebSocket(WSClientOut),
    WebRTC(WebRTCConnOut),
    Tick,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum InternOut {
    Network(NetworkOut),
    LocalSignal(LocalSignalIn),
    WebSocket(WSClientIn),
    WebRTC(WebRTCConnIn),
}

const UPDATE_INTERVAL: usize = 10;

pub(super) struct Messages {
    node_config: NodeConfig,
    get_update: usize,
    connections: HashMap<NodeID, IOConnection>,
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
                connections: HashMap::new(),
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
                if self.connected_only(&remote_node, &dir) {
                    out.push(NetworkOut::Disconnected(remote_node.clone()).into());
                    self.send_connections();
                }
                self.connection_state_set(remote_node.clone(), &dir, ConnectionState::Setup);
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
                if let Ok(s) = serde_yaml::to_string(&msg_nw) {
                    if !self.connected(&id) {
                        out.append(&mut self.connect(&id));
                    }
                    out.push(InternOut::WebRTC(WebRTCConnIn::Message(
                        id,
                        NCInput::Text(s),
                    )));
                }
                out
            }
            NetworkIn::StatsToWS(ss) => WSSignalMessageFromNode::NodeStats(ss.clone()).into(),
            NetworkIn::WSUpdateListRequest => WSSignalMessageFromNode::ListIDsRequest.into(),
            NetworkIn::Connect(id) => self.connect(&id),
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
        // log::info!("NC Message from {id}: {msg_nc}");
        match msg_nc {
            NCOutput::Connected(dir) => {
                let new_connection = !self.connected(&id);
                self.connection_state_set(id, &dir, ConnectionState::Connected);
                if new_connection {
                    vec![NetworkOut::Connected(id).into()]
                } else {
                    vec![]
                }
            }
            NCOutput::Disconnected(dir) => {
                let new_disconnection = self.connected_only(&id, &dir);
                self.connection_state_set(id, &dir, ConnectionState::None);
                if new_disconnection {
                    vec![NetworkOut::Disconnected(id).into()]
                } else {
                    vec![]
                }
            }
            NCOutput::Text(msg) => {
                if !self.connected(&id) {
                    log::warn!("Got text for unconnected node ({id})");
                }
                self.msg_wrapper(id, msg)
            }
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

    fn msg_local_signal(&mut self, msg: LocalSignalOut) -> Vec<InternOut> {
        match msg {
            LocalSignalOut::ToNode(id, node_msg) => {
                if self.connections.contains(&id) {
                    if let Ok(str) = NetworkWrapper::wrap_yaml(MODULE_NAME, &node_msg)
                        .and_then(|nw| serde_yaml::to_string(&nw).map_err(|e| anyhow::anyhow!(e)))
                    {
                        return vec![InternOut::WebRTC(WebRTCConnIn::Message(
                            id,
                            NCInput::Text(str),
                        ))];
                    }
                }
            }
            LocalSignalOut::NodeList(ids) => {
                return vec![];
            }
        }
        vec![]
    }

    /// Connect to the given node.
    fn connect(&mut self, dst: &U256) -> Vec<InternOut> {
        if self.connected(dst) {
            log::trace!("Already connected to {}", dst);
            vec![]
        } else {
            if self.connection_state_check(dst, &Direction::Outgoing, &ConnectionState::Setup) {
                // TODO: with the Helium browser, this message appears a lot. And then Helium doesn't
                // connect with webrtc.
                log::warn!(
                    "Asking to connect to {dst} while connection setup is in place - starting new setup"
                );
            } else {
                self.connection_state_set(*dst, &Direction::Outgoing, ConnectionState::Setup);
            }
            vec![InternOut::WebRTC(WebRTCConnIn::Connect(*dst))]
        }
    }

    /// Disconnects from a given node.
    fn disconnect(&mut self, dst: &U256) -> Vec<InternOut> {
        if !self.connected(dst) {
            log::trace!("Already disconnected from {}", dst);
            vec![]
        } else {
            self.connection_state_set(*dst, &Direction::Incoming, ConnectionState::None);
            self.connection_state_set(*dst, &Direction::Outgoing, ConnectionState::None);
            self.send_connections();
            return vec![
                NetworkOut::Disconnected(*dst).into(),
                InternOut::WebRTC(WebRTCConnIn::Message(*dst, NCInput::Disconnect)),
            ];
        }
    }

    fn send_connections(&mut self) {
        self.tx.clone().map(|tx| {
            tx.send(
                self.connections
                    .iter()
                    .filter_map(|(id, _)| self.connected(id).then(|| id.clone()))
                    .collect::<Vec<_>>(),
            )
            .is_err()
            .then(|| self.tx = None)
        });
    }
    fn connected_only(&self, node: &NodeID, dir: &Direction) -> bool {
        self.connection_state_check(node, dir, &ConnectionState::Connected)
            && !self.connection_state_check(node, &dir.other(), &ConnectionState::Connected)
    }

    fn connected(&self, node: &NodeID) -> bool {
        self.connection_state_check_any(node, &ConnectionState::Connected)
    }

    fn connection_state_check(
        &self,
        node: &NodeID,
        dir: &Direction,
        state: &ConnectionState,
    ) -> bool {
        self.connections.get(node).map(|ioc| ioc.get(dir)) == Some(state)
    }

    fn connection_state_check_any(&self, node: &NodeID, state: &ConnectionState) -> bool {
        self.connection_state_check(node, &Direction::Incoming, state)
            || self.connection_state_check(node, &Direction::Outgoing, state)
    }

    fn connection_state_set(&mut self, node: NodeID, dir: &Direction, state: ConnectionState) {
        // log::info!(
        //     "New connection-state for node {} in direction {:?} = {:?}",
        //     node,
        //     dir,
        //     state
        // );
        self.connections
            .entry(node)
            .and_modify(|ioc| ioc.set(dir, state.clone()))
            .or_insert(IOConnection::new(dir, state));
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ConnectionState {
    None,
    Setup,
    Connected,
}

#[derive(Debug, Clone)]
struct IOConnection {
    incoming: ConnectionState,
    outgoing: ConnectionState,
}

impl IOConnection {
    fn new(dir: &Direction, state: ConnectionState) -> Self {
        let mut ioc = IOConnection {
            incoming: ConnectionState::None,
            outgoing: ConnectionState::None,
        };
        ioc.set(dir, state);
        ioc
    }

    fn get(&self, dir: &Direction) -> &ConnectionState {
        match dir {
            Direction::Incoming => &self.incoming,
            Direction::Outgoing => &self.outgoing,
        }
    }

    fn set(&mut self, dir: &Direction, state: ConnectionState) {
        match dir {
            Direction::Incoming => self.incoming = state,
            Direction::Outgoing => self.outgoing = state,
        }
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
                InternIn::LocalSignal(msg) => self.msg_local_signal(msg),
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
            InternIn::LocalSignal(wsserver_in) => {
                write!(f, "InternalIn::Signal({:?})", wsserver_in)
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
