use log::{debug, info};

use std::{
    collections::HashMap,
    sync::mpsc::{channel, Receiver, Sender},
};

use super::{
    config::{NodeConfig, NodeInfo},
    network::connection_state::CSEnum,
};
use crate::{
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
    pub input_tx: Sender<LInput>,
    pub output_rx: Receiver<LOutput>,
    input_rx: Receiver<LInput>,
    output_tx: Sender<LOutput>,
    node_config: NodeConfig,
}

impl Logic {
    pub fn new(node_config: NodeConfig) -> Logic {
        let (input_tx, input_rx) = channel::<LInput>();
        let (output_tx, output_rx) = channel::<LOutput>();
        let stats = Stats {
            stats: HashMap::new(),
            last_stats: 0.,
            node_config: node_config.clone(),
            output_tx: output_tx.clone(),
        };
        Logic {
            node_config,
            stats,
            input_tx,
            input_rx,
            output_tx,
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
                LInput::ConnStat(id, dir, c, stm) => self.stats.update_connection_state(id, dir, c, stm),
            }
        }

        self.stats.send_stats()?;
        Ok(size)
    }

    fn rcv_sendv1(&mut self, from: U256, msg_id: U256, msg: MessageSendV1) -> Result<(), String> {
        debug!("got msg {:?} with id {:?} from {:?}", msg, msg_id, from);
        match msg {
            MessageSendV1::Ping() => self.stats.ping_rcv(from),
            MessageSendV1::TextIDsGet() => {}
            MessageSendV1::TextGet(_) => {}
            MessageSendV1::TextSet(_) => {}
        };
        Ok(())
    }

    fn rcv_replyv1(&self, from: U256, msg_id: U256, msg: MessageReplyV1) -> Result<(), String> {
        debug!("got msg {:?} with id {:?} from {:?}", msg, msg_id, from);
        match msg {
            MessageReplyV1::TextIDs(_) => {}
            MessageReplyV1::Text(_) => {}
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
        assert_eq!(2, logic.stats.stats.len(), "Should have two nodes now");

        sleep(Duration::from_millis(10));
        logic
            .input_tx
            .send(LInput::WebRTC(n1.id.clone(), "ping".to_string()))
            .map_err(|e| e.to_string())?;
        executor::block_on(logic.process())?;
        assert_eq!(1, logic.stats.stats.len(), "One node should disappear");
        Ok(())
    }
}
