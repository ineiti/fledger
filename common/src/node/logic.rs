use log::{debug, error, info};

use std::sync::mpsc::{channel, Receiver, Sender};

use super::{
    config::{NodeConfig, NodeInfo},
    network::connection_state::CSEnum,
};
use crate::{
    node::version::VERSION_SEMVER,
    signal::web_rtc::{ConnectionStateMap, NodeStat, WebRTCConnectionState},
    types::U256,
};

mod messages;
pub mod stats;
mod text_messages;
use messages::*;
use stats::*;
use text_messages::*;

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
        }
    }

    pub async fn process(&mut self) -> Result<usize, String> {
        let msgs: Vec<LInput> = self.input_rx.try_iter().collect();
        let size = msgs.len();
        for msg in msgs {
            match msg {
                LInput::WebRTC(id, msg) => self.rcv(id, msg)?,
                LInput::SetNodes(nodes) => self.stats.store_nodes(nodes),
                LInput::PingAll() => self.stats.ping_all()?,
                LInput::ConnStat(id, dir, c, stm) => {
                    self.stats.update_connection_state(id, dir, c, stm)
                }
            }
        }

        self.stats.send_stats()?;
        Ok(size)
    }

    fn rcv_sendv1(&mut self, from: U256, msg_id: U256, msg: MessageSendV1) -> Result<(), String> {
        debug!("got msg {:?} with id {:?} from {:?}", msg, msg_id, from);
        match msg {
            MessageSendV1::Ping() => self.stats.ping_rcv(&from),
            MessageSendV1::VersionGet() => {}
            MessageSendV1::TextMessage(msg) => {
                if let Some(reply) = self.text_messages.handle_send(msg)? {
                    let s = serde_json::to_string(&MessageReplyV1::TextMessage(reply))
                        .map_err(|e| e.to_string())?;
                    self._output_tx.send(LOutput::WebRTC(from, s));
                }
            }
        };
        Ok(())
    }

    fn rcv_replyv1(&self, from: U256, msg_id: U256, msg: MessageReplyV1) -> Result<(), String> {
        debug!("got msg {:?} with id {:?} from {:?}", msg, msg_id, from);
        match msg {
            MessageReplyV1::TextMessage(msg) => self.text_messages.handle_reply(&from, msg)?,
            MessageReplyV1::Version(v) => {
                if semver::Version::parse(&v).map_err(|e| e.to_string())?.major
                    > VERSION_SEMVER.major
                {
                    error!("Found node with higher major version - you should update yours, too!");
                }
            }
            MessageReplyV1::Ok() => {}
            MessageReplyV1::Error(_) => {}
        }
        Ok(())
    }

    fn rcv(&mut self, id: U256, msg_str: String) -> Result<(), String> {
        info!("Received message from WebRTC: {}", msg_str);
        let msg: Message = msg_str.into();
        match msg {
            Message::SendV1((msg_id, send)) => self.rcv_sendv1(id, msg_id, send),
            Message::ReplyV1((msg_id, rcv)) => self.rcv_replyv1(id, msg_id, rcv),
            Message::Unknown(s) => Err(s),
        }
    }
}
