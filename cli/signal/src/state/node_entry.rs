use log::error;
use std::{fmt, time::Instant};

use flnet::{
    config::NodeInfo,
    signal::{web_rtc::WSSignalMessageToNode, websocket::WebSocketConnection},
};
use flutils::nodeids::U256;

pub struct NodeEntry {
    pub conn: Box<dyn WebSocketConnection>,
    pub info: Option<NodeInfo>,
    pub last_seen: Instant,
    entry: U256,
}

impl fmt::Debug for NodeEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("NodeEntry: {:?}", self.info))?;
        Ok(())
    }
}

impl NodeEntry {
    pub fn new(entry: U256, conn: Box<dyn WebSocketConnection>) -> NodeEntry {
        let mut ne = NodeEntry {
            info: None,
            entry,
            conn,
            last_seen: Instant::now(),
        };
        let msg = serde_json::to_string(&WSSignalMessageToNode::Challenge(1u64, ne.entry)).unwrap();
        if let Err(e) = ne.conn.send(msg) {
            error!("while sending challenge: {}", e);
        }
        ne
    }
}
