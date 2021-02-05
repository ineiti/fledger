use crate::{config::NodeInfo, web_rtc::WebRTCSpawner};
use crate::{ext_interface::Logger, web_rtc::WSSignalMessage};

use crate::types::U256;
use crate::websocket::WebSocketConnection;
use std::sync::{Arc, Mutex};

pub struct Network {
    intern: Arc<Mutex<Intern>>,
    logger: Box<dyn Logger>,
}

mod intern;
mod node_connection;
use intern::Intern;

pub type WebRTCReceive = Arc<Mutex<Box<dyn Fn(U256, String)>>>;

/// Network combines a websocket to connect to the signal server with
/// a WebRTC trait to connect to other nodes.
/// It supports setting up automatic connetions to other nodes.
impl Network {
    pub fn new(
        ws: Box<dyn WebSocketConnection>,
        web_rtc: WebRTCSpawner,
        web_rtc_rcv: WebRTCReceive,
        logger: Box<dyn Logger>,
        node_info: NodeInfo,
    ) -> Network {
        let net = Network {
            intern: Intern::new(ws, web_rtc, web_rtc_rcv, logger.clone(), node_info),
            logger,
        };
        net
    }

    /// Sending strings to other nodes. If the connection already exists,
    /// it will be used to send the string over.
    /// Else the signalling server will be contacted, a webrtc connection will
    /// be created, and then the message will be sent over.
    /// During the setup of a new connection, the message is stored in a queue.
    /// So in the case of a new connection, the 'send' method returns even before the
    /// message is actually sent
    pub async fn send(&self, dst: &U256, msg: String) -> Result<(), String> {
        self.logger.info(&format!("Sending to: {}", dst));
        let mut int = self.intern.lock().unwrap();
        int.send(dst, msg).await
    }

    pub fn clear_nodes(&self) {
        self.logger.info(&format!("Clearing nodes"));
        self.intern
            .lock()
            .unwrap()
            .send_ws(WSSignalMessage::ClearNodes);
    }

    pub fn update_node_list(&self) {
        self.logger.info(&format!("Updating node list"));
        Arc::clone(&self.intern).lock().unwrap().update_node_list();
    }

    pub fn get_list(&self) -> Vec<NodeInfo> {
        self.logger.info(&format!("getting list"));
        Arc::clone(&self.intern).lock().unwrap().list.clone()
    }
}
