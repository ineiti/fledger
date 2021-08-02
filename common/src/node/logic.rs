use log::{error, debug, trace};

use std::sync::mpsc::{channel, Receiver, Sender};

use super::{
    config::{NodeConfig, NodeInfo},
    network::connection_state::CSEnum,
};
use crate::{
    node::version::VERSION_STRING,
    signal::web_rtc::{ConnectionStateMap, NodeStat, WebRTCConnectionState},
    types::U256,
};

mod messages;
pub mod stats;
pub mod text_messages;
use messages::*;
use stats::*;
pub use text_messages::*;

#[derive(Debug)]
pub enum LInput {
    WebRTC(U256, String),
    SetNodes(Vec<NodeInfo>),
    PingAll(),
    ConnStat(
        U256,
        WebRTCConnectionState,
        CSEnum,
        Option<ConnectionStateMap>,
    ),
    AddMessage(String),
}

#[derive(Debug)]
pub enum LOutput {
    WebRTC(U256, String),
    SendStats(Vec<NodeStat>),
}

pub struct Logic {
    pub stats: Stats,
    pub text_messages: TextMessages,
    pub input_tx: Sender<LInput>,
    pub output_rx: Receiver<LOutput>,
    input_rx: Receiver<LInput>,
    _output_tx: Sender<LOutput>,
    _node_config: NodeConfig,
    counter: u32,
}

impl Logic {
    pub fn new(node_config: NodeConfig) -> Logic {
        let (input_tx, input_rx) = channel::<LInput>();
        let (_output_tx, output_rx) = channel::<LOutput>();
        let stats = Stats::new(node_config.clone(), _output_tx.clone());
        Logic {
            _node_config: node_config.clone(),
            stats,
            text_messages: TextMessages::new(_output_tx.clone(), node_config.clone()),
            input_tx,
            input_rx,
            _output_tx,
            output_rx,
            counter: 0,
        }
    }

    pub async fn process(&mut self) -> Result<usize, String> {
        let msgs: Vec<LInput> = self.input_rx.try_iter().collect();
        let size = msgs.len();
        for msg in msgs {
            match msg {
                LInput::WebRTC(id, msg) => self.rcv(id, msg)?,
                LInput::SetNodes(nodes) => {
                    self.stats.store_nodes(nodes.clone());
                    self.text_messages.update_nodes(nodes).map_err(|e| e.to_string())?;
                }
                LInput::PingAll() => self.stats.ping_all()?,
                LInput::ConnStat(id, dir, c, stm) => {
                    self.stats.update_connection_state(id, dir, c, stm)
                }
                LInput::AddMessage(msg) => self
                    .text_messages
                    .add_message(msg)
                    .map_err(|e| e.to_string())?,
            }
        }
        // TODO: don't use a counter here, as the `process` is called a variable number
        // of times per second.
        self.counter += 1;
        if self.counter % 100 == 0 {
            self.text_messages
                .update_messages()
                .map_err(|e| e.to_string())?;
        }
        self.stats.send_stats()?;
        Ok(size)
    }

    fn rcv_msg(&mut self, from: U256, msg: MessageV1) -> Result<(), String> {
        debug!("got msg {} from {:?}", msg, from);
        match msg {
            MessageV1::Ping() => self.stats.ping_rcv(&from),
            MessageV1::VersionGet() => {}
            MessageV1::TextMessage(msg) => self
                .text_messages
                .handle_msg(&from, msg)
                .map_err(|e| e.to_string())?,
            MessageV1::Version(v) => {
                let version_semver = semver::Version::parse(VERSION_STRING).unwrap();
                if semver::Version::parse(&v).map_err(|e| e.to_string())?.major
                    > version_semver.major
                {
                    error!("Found node with higher major version - you should update yours, too!");
                }
            }
            MessageV1::Ok() => {}
            MessageV1::Error(_) => {}
        }
        Ok(())
    }

    fn rcv(&mut self, id: U256, msg_str: String) -> Result<(), String> {
        trace!("Received message from WebRTC: {}", msg_str);
        let msg: Message = msg_str.into();
        match msg {
            Message::V1(send) => self.rcv_msg(id, send),
            Message::Unknown(s) => Err(s),
        }
    }
}
