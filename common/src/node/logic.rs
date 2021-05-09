use log::info;

#[cfg(target_arch = "wasm32")]
fn now() -> f64 {
    use js_sys::Date;
    Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
fn now() -> f64 {
    use chrono::Utc;
    Utc::now().timestamp_millis() as f64
}

use std::{
    collections::HashMap,
    sync::mpsc::{channel, Receiver, Sender},
};

use super::{
    config::{NodeConfig, NodeInfo},
    network::connection_state::CSEnum,
    version::VERSION_STRING,
};
use crate::{
    signal::web_rtc::{ConnectionStateMap, NodeStat, WebRTCConnectionState},
    types::U256,
};

#[derive(Debug)]
pub enum LInput {
    WebRTC(U256, String),
    SetNodes(Vec<NodeInfo>),
    PingAll(String),
    ConnStat(
        U256,
        WebRTCConnectionState,
        CSEnum,
        Option<ConnectionStateMap>,
    ),
}

#[derive(Debug)]
pub enum LOutput {
    WebRTC(U256, String),
    SendStats(Vec<NodeStat>),
}

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
pub struct Stat {
    pub node_info: Option<NodeInfo>,
    pub ping_rx: u32,
    pub ping_tx: u32,
    pub last_contact: f64,
    pub incoming: ConnState,
    pub outgoing: ConnState,
    pub client_info: String,
}

impl Stat {
    pub fn new(node_info: Option<NodeInfo>) -> Stat {
        Stat {
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

pub struct Logic {
    pub stats: HashMap<U256, Stat>,
    pub input_tx: Sender<LInput>,
    pub output_rx: Receiver<LOutput>,
    input_rx: Receiver<LInput>,
    output_tx: Sender<LOutput>,
    node_config: NodeConfig,
    last_stats: f64,
}

impl Logic {
    pub fn new(node_config: NodeConfig) -> Logic {
        let (input_tx, input_rx) = channel::<LInput>();
        let (output_tx, output_rx) = channel::<LOutput>();
        Logic {
            node_config,
            stats: HashMap::new(),
            input_tx,
            input_rx,
            output_tx,
            output_rx,
            last_stats: 0.,
        }
    }

    pub async fn process(&mut self) -> Result<usize, String> {
        let msgs: Vec<LInput> = self.input_rx.try_iter().collect();
        let size = msgs.len();
        for msg in msgs {
            match msg {
                LInput::WebRTC(id, msg) => self.rcv(id, msg),
                LInput::SetNodes(nodes) => self.store_nodes(nodes),
                LInput::PingAll(msg) => self.ping_all(msg)?,
                LInput::ConnStat(id, dir, c, stm) => self.update_connection_state(id, dir, c, stm),
            }
        }

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
        Ok(size)
    }

    fn update_connection_state(
        &mut self,
        id: U256,
        dir: WebRTCConnectionState,
        st: CSEnum,
        state: Option<ConnectionStateMap>,
    ) {
        self.stats
            .entry(id.clone())
            .or_insert_with(|| Stat::new(None));
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

    fn store_nodes(&mut self, nodes: Vec<NodeInfo>) {
        for ni in nodes {
            let s = self
                .stats
                .entry(ni.id.clone())
                .or_insert_with(|| Stat::new(None));
            s.node_info = Some(ni);
        }
    }

    fn ping_all(&mut self, msg: String) -> Result<(), String> {
        for stat in self.stats.iter_mut() {
            if let Some(ni) = stat.1.node_info.as_ref() {
                if self.node_config.our_node.id != ni.id {
                    self.output_tx
                        .send(LOutput::WebRTC(ni.id.clone(), msg.clone()))
                        .map_err(|e| e.to_string())?;
                    stat.1.ping_tx += 1;
                }
            }
        }
        Ok(())
    }

    fn rcv(&mut self, id: U256, msg: String) {
        info!("Received message from WebRTC: {}", msg);
        self.stats
            .entry(id.clone())
            .or_insert_with(|| Stat::new(None));
        self.stats.entry(id.clone()).and_modify(|s| {
            s.last_contact = now();
            s.ping_rx += 1;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{LInput, Logic};
    use crate::node::config::{NodeConfig, NodeInfo};
    use flexi_logger::Logger;
    use futures::executor;
    use std::{thread::sleep, time::Duration};

    #[test]
    /// Starts a Logic with two nodes, then receives ping only from one node.
    /// The other node should be removed from the stats-list.
    fn cleanup_stale_nodes() -> Result<(), String> {
        Logger::with_str("debug").start().unwrap();

        let n1 = NodeInfo::new();
        let n2 = NodeInfo::new();
        let mut nc = NodeConfig::new("".to_string())?;
        nc.send_stats = Some(1f64);
        nc.stats_ignore = Some(2f64);

        let mut logic = Logic::new(nc);
        logic
            .input_tx
            .send(LInput::SetNodes(vec![n1.clone(), n2.clone()]))
            .map_err(|e| e.to_string())?;
        executor::block_on(logic.process())?;
        assert_eq!(2, logic.stats.len(), "Should have two nodes now");

        sleep(Duration::from_millis(10));
        logic
            .input_tx
            .send(LInput::WebRTC(n1.id.clone(), "ping".to_string()))
            .map_err(|e| e.to_string())?;
        executor::block_on(logic.process())?;
        assert_eq!(1, logic.stats.len(), "One node should disappear");
        Ok(())
    }
}
