use crate::config::NodeInfo;
use std::sync::{Arc, Mutex};

pub struct Logic {
    node_info: NodeInfo,
}

impl Logic {
    pub fn new(node_info: NodeInfo) -> Arc<Mutex<Logic>> {
        Arc::new(Mutex::new(Logic { node_info }))
    }
}
