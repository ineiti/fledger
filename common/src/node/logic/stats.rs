use std::{collections::HashMap, sync::mpsc::Sender};

#[cfg(target_arch = "wasm32")]
pub fn now() -> f64 {
    use js_sys::Date;
    Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn now() -> f64 {
    use chrono::Utc;
    Utc::now().timestamp_millis() as f64
}

use super::{
    messages::{Message, MessageSendV1},
    LOutput,
};
use crate::{
    node::{
        config::{NodeConfig, NodeInfo},
        network::connection_state::CSEnum,
        version::VERSION_STRING,
    },
    signal::web_rtc::{ConnectionStateMap, NodeStat, WebRTCConnectionState},
    types::U256,
};

#[derive(Debug, PartialEq, Clone)]
pub enum ConnState {
    Idle,
    Setup,
    Connected,
    TURN,
    STUN,
    Host,
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
    pub fn new(node_info: Option<NodeInfo>) -> Self {
        Self {
            node_info,
            ping_rx: 0,
            ping_tx: 0,
            last_contact: now(),
            incoming: ConnState::Idle,
            outgoing: ConnState::Idle,
            client_info: "N/A".to_string(),
        }
    }
}

pub struct Stats {
    pub stats: HashMap<U256, StatNode>,
    pub last_stats: f64,
    pub node_config: NodeConfig,
    pub output_tx: Sender<LOutput>,
}

impl Stats {
    pub fn send_stats(&mut self) -> Result<(), String> {
        // Send statistics to the signalling server
        if now() > self.last_stats + self.node_config.send_stats.unwrap() {
            self.last_stats = now();
            let expired: Vec<U256> = self
                .stats
                .iter()
                .filter(|(_k, v)| now() - v.last_contact > self.node_config.stats_ignore.unwrap())
                .map(|(k, _v)| k.clone())
                .collect();
            for k in expired {
                self.stats.remove(&k);
            }

            let stats: Vec<NodeStat> = self
                .stats
                .iter()
                // Ignore our node and nodes that are inactive
                .filter(|(k, _v)| k != &&self.node_config.our_node.id)
                .map(|(k, v)| NodeStat {
                    id: k.clone(),
                    version: VERSION_STRING.to_string(),
                    ping_ms: 0u32,
                    ping_rx: v.ping_rx,
                })
                .collect();
            self.output_tx
                .send(LOutput::SendStats(stats))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn update_connection_state(
        &mut self,
        id: U256,
        dir: WebRTCConnectionState,
        st: CSEnum,
        state: Option<ConnectionStateMap>,
    ) {
        self.stats
            .entry(id.clone())
            .or_insert_with(|| StatNode::new(None));
        self.stats.entry(id.clone()).and_modify(|s| {
            let cs = match st {
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
            };
            if dir == WebRTCConnectionState::Initializer {
                s.outgoing = cs;
            } else {
                s.incoming = cs;
            }
        });
    }

    pub fn store_nodes(&mut self, nodes: Vec<NodeInfo>) {
        for ni in nodes {
            let s = self
                .stats
                .entry(ni.id.clone())
                .or_insert_with(|| StatNode::new(None));
            s.node_info = Some(ni);
        }
    }

    pub fn ping_all(&mut self) -> Result<(), String> {
        for stat in self.stats.iter_mut() {
            if let Some(ni) = stat.1.node_info.as_ref() {
                if self.node_config.our_node.id != ni.id {
                    let msg_send = Message::SendV1((U256::rnd(), MessageSendV1::Ping()));
                    self.output_tx
                        .send(LOutput::WebRTC(ni.id.clone(), msg_send.to_string()?))
                        .map_err(|e| e.to_string())?;
                    stat.1.ping_tx += 1;
                }
            }
        }
        Ok(())
    }

    pub fn ping_rcv(&mut self, from: U256) {
        self.stats
            .entry(from.clone())
            .or_insert_with(|| StatNode::new(None));
        self.stats.entry(from.clone()).and_modify(|s| {
            s.last_contact = now();
            s.ping_rx += 1;
        });
    }
}
