use super::{config::NodeInfo, ext_interface::Logger, types::U256};
use std::{
    collections::HashMap,
    sync::mpsc::{channel, Receiver, Sender},
};

#[derive(Debug)]
pub enum LInput {
    WebRTC(U256, String),
}

#[derive(Debug)]
pub enum LOutput {
    WebRTC(U256, String),
}

pub struct Logic {
    pub pings: HashMap<U256, u64>,
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
            pings: HashMap::new(),
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
            }
        }
        Ok(())
    }

    fn rcv(&mut self, id: U256, msg: String) {
        self.logger.info(&format!("Got msg {} from id {}", msg, id));
        self.pings.entry(id.clone()).or_insert_with(|| 0);
        self.pings.entry(id.clone()).and_modify(|c| *c = *c + 1u64);
    }
}
