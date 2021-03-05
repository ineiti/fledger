use super::{config::NodeInfo, ext_interface::Logger, types::U256};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

pub struct Logic {
    node_info: NodeInfo,
    logger: Box<dyn Logger>,
    pub pings: HashMap<U256, u64>,
}

impl Logic {
    pub fn new(node_info: NodeInfo, logger: Box<dyn Logger>) -> Arc<Mutex<Logic>> {
        Arc::new(Mutex::new(Logic {
            node_info,
            logger,
            pings: HashMap::new(),
        }))
    }

    pub fn rcv(&mut self, id: U256, msg: String) {
        self.logger.info(&format!("Got msg {} from id {}", msg, id));
        self.pings.entry(id.clone()).or_insert_with(||0);
        self.pings.entry(id.clone()).and_modify(|c| *c = *c + 1u64);
    }
}
