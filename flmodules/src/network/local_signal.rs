//! LocalSignal is a local signalling server which can setup WebRTC connections
//! of other nodes connected to it.
//! If node A has a connection with nodes B and C, and node B only has a connection
//! with node A, node B can use node A to set up a WebRTC connection with node C.
//! Of course this still needs a STUN server to get the network identities
//! of node C and node B.

use std::collections::HashSet;

use bimap::BiMap;
use flarch::{
    add_translator_direct, add_translator_link,
    broker::{Broker, SubsystemHandler},
    nodeids::NodeID,
    platform_async_trait,
    web_rtc::websocket::{WSServerIn, WSServerOut},
};

use crate::network::signal::{SignalIn, SignalOut, WSSignalMessageFromNode, WSSignalMessageToNode};

use super::signal::{SignalConfig, SignalServer};

pub type BrokerLocalSignal = Broker<LocalSignalIn, LocalSignalOut>;

#[derive(Debug, Clone, PartialEq)]
pub enum LocalSignalIn {
    FromNode(NodeID, WSSignalMessageFromNode),
    TimerSec,
    TimerMin,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LocalSignalOut {
    ToNode(NodeID, WSSignalMessageToNode),
    NodeList(Vec<NodeID>),
}

pub struct LocalSignal {
    ws_connections: BiMap<NodeID, usize>,
    nodes: HashSet<NodeID>,
}

impl LocalSignal {
    pub async fn start() -> anyhow::Result<BrokerLocalSignal> {
        let mut intern_broker = Broker::new();
        intern_broker
            .add_handler(Box::new(LocalSignal {
                ws_connections: BiMap::new(),
                nodes: HashSet::new(),
            }))
            .await?;

        let signal_broker = SignalServer::new(SignalConfig {
            ttl_minutes: 2,
            system_realm: None,
            max_list_len: None,
        })
        .await?;

        add_translator_link!(
            intern_broker,
            signal_broker,
            InternOut::Signal,
            InternIn::Signal
        );

        let local_signal_broker = Broker::new();
        add_translator_direct!(
            intern_broker,
            local_signal_broker.clone(),
            InternOut::LocalSignal,
            InternIn::LocalSignal
        );
        Ok(local_signal_broker)
    }

    fn msg_local(&mut self, msg: LocalSignalIn) -> Vec<InternOut> {
        let mut out = vec![];
        match msg {
            LocalSignalIn::FromNode(id, node_msg) => {
                if !self.ws_connections.contains_left(&id) {
                    let ws_conn = self.ws_connections.len() + 1;
                    self.ws_connections.insert(id.clone(), ws_conn);
                    out.push(InternOut::Signal(SignalIn::WSServer(
                        WSServerOut::NewConnection(ws_conn),
                    )));
                }
                if let Some(ws_conn) = self.ws_connections.get_by_left(&id) {
                    out.push(InternOut::Signal(SignalIn::WSServer(WSServerOut::Message(
                        *ws_conn,
                        node_msg.to_string(),
                    ))));
                }
            }
            LocalSignalIn::TimerMin => out.push(InternOut::Signal(SignalIn::Timer)),
            LocalSignalIn::TimerSec => {}
        }
        out
    }

    fn msg_from_signal(&mut self, msg: SignalOut) -> Vec<InternOut> {
        match msg {
            SignalOut::NewNode(id) => {
                self.nodes.insert(id);
                return vec![InternOut::LocalSignal(LocalSignalOut::NodeList(
                    self.nodes.iter().cloned().collect(),
                ))];
            }
            SignalOut::WSServer(ws_in) => {
                if let WSServerIn::Message(ws_conn, msg_ws_str) = ws_in {
                    if let Some(id) = self.ws_connections.get_by_right(&ws_conn) {
                        if let Ok(msg_ws) =
                            serde_json::from_str::<WSSignalMessageToNode>(&msg_ws_str)
                        {
                            return vec![InternOut::LocalSignal(LocalSignalOut::ToNode(
                                *id, msg_ws,
                            ))];
                        }
                    }
                }
            }
            _ => {}
        }
        vec![]
    }
}

#[derive(Debug, Clone, PartialEq)]
enum InternIn {
    LocalSignal(LocalSignalIn),
    Signal(SignalOut),
}

#[derive(Debug, Clone, PartialEq)]
enum InternOut {
    LocalSignal(LocalSignalOut),
    Signal(SignalIn),
}

#[platform_async_trait()]
impl SubsystemHandler<InternIn, InternOut> for LocalSignal {
    async fn messages(&mut self, msgs: Vec<InternIn>) -> Vec<InternOut> {
        let mut out = vec![];
        for msg in msgs {
            out.extend(match msg {
                InternIn::LocalSignal(ls_in) => self.msg_local(ls_in),
                InternIn::Signal(s_out) => self.msg_from_signal(s_out),
            });
        }
        vec![]
    }
}
