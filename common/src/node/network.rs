use super::{config::NodeInfo, ext_interface::Logger, types::U256};
use crate::signal::{
    web_rtc::{WSSignalMessage, WebRTCSpawner},
    websocket::WebSocketConnection,
};

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
        if let Ok(mut int) = self.intern.try_lock() {
            return int.send(dst, msg).await;
        } else {
            return Err("Couldn't take lock of intern".to_string());
        }
    }

    pub fn clear_nodes(&self) -> Result<(), String> {
        self.logger.info(&format!("Clearing nodes"));
        if let Ok(mut int) = self.intern.try_lock() {
            int.send_ws(WSSignalMessage::ClearNodes);
            Ok(())
        } else {
            Err("Couldn't get lock on int in clear_nodes".to_string())
        }
    }

    pub fn update_node_list(&self) -> Result<(), String> {
        self.logger.info(&format!("Updating node list"));
        if let Ok(mut int) = self.intern.try_lock() {
            int.update_node_list();
            Ok(())
        } else {
            Err("Couldn't get lock on int in update_node_list".to_string())
        }
    }

    pub fn get_list(&self) -> Result<Vec<NodeInfo>, String> {
        if let Ok(int) = self.intern.try_lock() {
            Ok(int.list.clone())
        } else {
            Err("Couldn't get lock on int in get_list".to_string())
        }
    }
}
