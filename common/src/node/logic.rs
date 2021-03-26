use super::{config::{NodeConfig, NodeInfo, NODE_VERSION}, ext_interface::Logger, network::connection_state::CSEnum, types::U256};
use crate::signal::web_rtc::{ConnectionStateMap, NodeStat, WebRTCConnectionState};
use js_sys::Date;
use std::{
    collections::HashMap,
    sync::mpsc::{channel, Receiver, Sender},
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
            last_contact: Date::now(),
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
    logger: Box<dyn Logger>,
    last_stats: f64,
}

impl Logic {
    pub fn new(node_config: NodeConfig, logger: Box<dyn Logger>) -> Logic {
        let (input_tx, input_rx) = channel::<LInput>();
        let (output_tx, output_rx) = channel::<LOutput>();
        Logic {
            node_config,
            logger,
            stats: HashMap::new(),
            input_tx,
            input_rx,
            output_tx,
            output_rx,
            last_stats: 0.,
        }
    }

    pub async fn process(&mut self) -> Result<(), String> {
        let msgs: Vec<LInput> = self.input_rx.try_iter().collect();
        for msg in msgs {
            match msg {
                LInput::WebRTC(id, msg) => self.rcv(id, msg),
                LInput::SetNodes(nodes) => self.store_nodes(nodes),
                LInput::PingAll(msg) => self.ping_all(msg)?,
                LInput::ConnStat(id, dir, c, stm) => self.update_connection_state(id, dir, c, stm),
            }
        }
        // Send statistics to the signalling server
        if Date::now() > self.last_stats + self.node_config.send_stats.unwrap() {
            self.last_stats = Date::now();
            let stats: Vec<NodeStat> = self
                .stats
                .iter()
                // Ignore our node and nodes that are inactive
                .filter(|(k, v)| k != &&self.node_config.our_node.id && Date::now() - v.last_contact < self.node_config.stats_ignore.unwrap())
                .map(|(k, v)| NodeStat {
                    id: k.clone(),
                    version: NODE_VERSION,
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
                CSEnum::Connected => {
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
        self.stats
            .entry(id.clone())
            .or_insert_with(|| Stat::new(None));
        self.stats.entry(id.clone()).and_modify(|s| {
            s.last_contact = Date::now();
            s.ping_rx += 1;
        });
    }
}
