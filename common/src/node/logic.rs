use super::{
    config::NodeInfo, ext_interface::Logger, network::connection_state::CSEnum, types::U256,
};
use crate::signal::web_rtc::{ConnectionStateMap, WebRTCConnectionState};
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
}

#[derive(Debug, PartialEq, Clone)]
pub enum ConnState {
    Idle,
    Setup,
    Connected,
    TURN,
    STUN,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Stat {
    pub node_info: Option<NodeInfo>,
    pub ping_rx: u64,
    pub ping_tx: u64,
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
    node_info: NodeInfo,
    logger: Box<dyn Logger>,
}

impl Logic {
    pub fn new(node_info: NodeInfo, logger: Box<dyn Logger>) -> Logic {
        let (input_tx, input_rx) = channel::<LInput>();
        let (output_tx, output_rx) = channel::<LOutput>();
        Logic {
            node_info,
            logger,
            stats: HashMap::new(),
            input_tx,
            input_rx,
            output_tx,
            output_rx,
        }
    }

    pub async fn process(&mut self) -> Result<(), String> {
        let msgs: Vec<LInput> = self.input_rx.try_iter().collect();
        for msg in msgs {
            // self.logger
            //     .info(&format!("dbg: Logic::process got {:?}", msg));
            match msg {
                LInput::WebRTC(id, msg) => self.rcv(id, msg),
                LInput::SetNodes(nodes) => self.store_nodes(nodes),
                LInput::PingAll(msg) => self.ping_all(msg)?,
                LInput::ConnStat(id, dir, c, stm) => self.update_connection_state(id, dir, c, stm),
            }
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
                CSEnum::Connected => ConnState::Connected,
            };
            if dir == WebRTCConnectionState::Initializer {
                s.outgoing = cs;
            } else {
                s.incoming = cs;
            }
        });
        if let Some(s) = state {
            self.logger.info(&format!("Got CSM: {:?}", s));
        }
    }

    fn store_nodes(&mut self, nodes: Vec<NodeInfo>) {
        for ni in nodes {
            self.stats
                .entry(ni.public.clone())
                .or_insert_with(|| Stat::new(Some(ni)));
        }
    }

    fn ping_all(&mut self, msg: String) -> Result<(), String> {
        for stat in self.stats.iter_mut() {
            if let Some(ni) = stat.1.node_info.as_ref() {
                if self.node_info.public != ni.public {
                    self.output_tx
                        .send(LOutput::WebRTC(ni.public.clone(), msg.clone()))
                        .map_err(|e| e.to_string())?;
                    stat.1.ping_tx += 1;
                }
            }
        }
        Ok(())
    }

    fn rcv(&mut self, id: U256, msg: String) {
        self.logger.info(&format!("Got msg {} from id {}", msg, id));
        self.stats
            .entry(id.clone())
            .or_insert_with(|| Stat::new(None));
        self.stats.entry(id.clone()).and_modify(|s| {
            s.last_contact = Date::now();
            s.ping_rx += 1;
        });
    }
}
