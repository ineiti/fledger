use core::f64;
/// StatNode handles the statistics that are taken for the nodes. 

use std::{collections::HashMap, sync::mpsc::Sender};

use crate::{node::{config::NodeInfo, logic::{LOutput, messages::{Message, MessageSendV1}}, network::connection_state::CSEnum, version::VERSION_STRING}, signal::web_rtc::{ConnectionStateMap, NodeStat, WebRTCConnectionState}, types::{now, U256}};

#[derive(Debug, PartialEq, Clone)]
pub enum ConnState {
    Idle,
    Setup,
    Connected,
    TURN,
    STUN,
    Host,
}

impl ConnState {
    pub fn from_states(st: CSEnum, state: Option<ConnectionStateMap>) -> Self {
        match st {
            CSEnum::Idle => ConnState::Idle,
            CSEnum::Setup => ConnState::Setup,
            CSEnum::HasDataChannel => {
                if let Some(state_value) = state {
                    match state_value.type_local {
                        crate::signal::web_rtc::ConnType::Unknown => ConnState::Connected,
                        crate::signal::web_rtc::ConnType::Host => ConnState::Host,
                        crate::signal::web_rtc::ConnType::STUNPeer => ConnState::STUN,
                        crate::signal::web_rtc::ConnType::STUNServer => ConnState::STUN,
                        crate::signal::web_rtc::ConnType::TURN => ConnState::TURN,
                    }
                } else {
                    ConnState::Connected
                }
            }
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct StatNode {
    pub node_info: Option<NodeInfo>,
    pub ping_rx: u32,
    pub ping_tx: u32,
    pub last_contact: f64,
    pub incoming: ConnState,
    pub outgoing: ConnState,
    pub client_info: String,
}

impl StatNode {
    pub fn new(node_info: Option<NodeInfo>, last_contact: f64) -> Self {
        Self {
            node_info,
            ping_rx: 0,
            ping_tx: 0,
            last_contact,
            incoming: ConnState::Idle,
            outgoing: ConnState::Idle,
            client_info: "N/A".to_string(),
        }
    }
}

pub struct StatNodes{
    nodes: HashMap<U256, StatNode>,
    last_stats: f64,
}

impl StatNodes {
    pub fn new() -> Self {
        StatNodes{nodes: HashMap::new(), last_stats: 0.}
    }

    pub fn tick(&mut self, send_stats: f64) -> bool{
        let now_here = now();
        if now_here > self.last_stats + send_stats {
            self.last_stats = now_here;
            return true;
        }
        false
    }

    pub fn expire(&mut self, stats_ignore: f64) {
        let expired: Vec<U256> = self
            .nodes
            .iter()
            .filter(|(_k, v)| self.last_stats - v.last_contact > stats_ignore)
            .map(|(k, _v)| k.clone())
            .collect();
        for k in expired {
            self.nodes.remove(&k);
        }
    }

    pub fn collect(&self, our_id: &U256) -> Vec<NodeStat> {
        self.nodes
            .iter()
            // Ignore our node and nodes that are inactive
            .filter(|(k, _v)| k != &our_id)
            .map(|(k, v)| NodeStat {
                id: k.clone(),
                version: VERSION_STRING.to_string(),
                ping_ms: 0u32,
                ping_rx: v.ping_rx,
            })
            .collect()
    }

    pub fn upsert(&mut self, id: &U256, dir: WebRTCConnectionState, cs: ConnState) {
        self.nodes
            .entry(id.clone())
            .or_insert_with(|| StatNode::new(None, now()));
        self.nodes.entry(id.clone()).and_modify(|s| {
            if dir == WebRTCConnectionState::Initializer {
                s.outgoing = cs;
            } else {
                s.incoming = cs;
            }
        });
    }

    pub fn update(&mut self, ni: NodeInfo) {
        let s = self
            .nodes
            .entry(ni.id.clone())
            .or_insert_with(|| StatNode::new(None, now()));
        s.node_info = Some(ni);
    }

    pub fn ping_all(&mut self, our_id: &U256, out: Sender<LOutput>) -> Result<(), String> {
        for stat in self.nodes.iter_mut() {
            if let Some(ni) = stat.1.node_info.as_ref() {
                if our_id != &ni.id {
                    let msg_send = Message::SendV1((U256::rnd(), MessageSendV1::Ping()));
                    out.send(LOutput::WebRTC(ni.id.clone(), msg_send.to_string()?))
                        .map_err(|e| e.to_string())?;
                    stat.1.ping_tx += 1;
                }
            }
        }
        Ok(())
    }

    pub fn ping_rcv(&mut self, from: &U256) {
        self.nodes
            .entry(from.clone())
            .or_insert_with(|| StatNode::new(None, now()));
        self.nodes.entry(from.clone()).and_modify(|s| {
            s.last_contact = now();
            s.ping_rx += 1;
        });
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn clone(&self) -> HashMap<U256, StatNode> {
        self.nodes.clone()
    }
}
