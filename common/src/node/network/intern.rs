use crate::node::{
    config::NodeInfo,
    ext_interface::Logger,
    types::U256,
    network::{WebRTCReceive, node_connection::NodeConnection},
};
use crate::signal::{
    web_rtc::{
        MessageAnnounce, PeerInfo, PeerMessage, WSSignalMessage, WebRTCSpawner, WebSocketMessage,
    },
    websocket::{WSMessage, WebSocketConnection},
};

use std::sync::Mutex;
use std::{collections::HashMap, sync::Arc};

pub struct Intern {
    ws: Box<dyn WebSocketConnection>,
    web_rtc: Arc<Mutex<WebRTCSpawner>>,
    web_rtc_rcv: WebRTCReceive,
    connections: HashMap<U256, NodeConnection>,
    logger: Box<dyn Logger>,
    node_info: NodeInfo,
    pub list: Vec<NodeInfo>,
}

impl Intern {
    /// Returns a new Arc<Mutex<Intern>> wired up to process incoming messages
    /// through the WebSocket.
    pub fn new(
        ws: Box<dyn WebSocketConnection>,
        web_rtc: WebRTCSpawner,
        web_rtc_rcv: WebRTCReceive,
        logger: Box<dyn Logger>,
        node_info: NodeInfo,
    ) -> Arc<Mutex<Intern>> {
        let int = Arc::new(Mutex::new(Intern {
            ws,
            web_rtc: Arc::new(Mutex::new(web_rtc)),
            web_rtc_rcv,
            connections: HashMap::new(),
            logger: logger.clone(),
            node_info,
            list: vec![],
        }));
        let int_cl = Arc::clone(&int);
        let msg_queue = Arc::new(Mutex::new(vec![]));
        int.lock()
            .unwrap()
            .ws
            .set_cb_wsmessage(Box::new(move |msg| {
                // This callback is somewhat tricky, because the msg_cb is async and thus can
                // block, letting a new message arrive.
                // But wasm doesn't support _real_ mutexes, only ones that don't block.
                // So this Closure uses the msg_queue to buffer incoming messages.
                // And as long as there are messages, the one having the lock will process them.
                let int_cl_square = Arc::clone(&int_cl);
                let mq = Arc::clone(&msg_queue);

                wasm_bindgen_futures::spawn_local(async move {
                    {
                        // Release the lock as fast as possible.
                        mq.lock().unwrap().push(msg)
                    }
                    if let Ok(mut cl) = int_cl_square.try_lock() {
                        loop {
                            // drain the message queue, as we shouldn't hold the lock.
                            let msgs: Vec<WSMessage> = { mq.lock().unwrap().drain(..).collect() };
                            if msgs.len() == 0 {
                                break;
                            }
                            for msg in msgs.into_iter() {
                                cl.msg_cb(msg).await;
                            }
                        }
                    }
                });
            }));
        int
    }

    async fn msg_cb(&mut self, msg: WSMessage) {
        // self.logger.info("parsing message");
        match msg {
            WSMessage::MessageString(s) => match WebSocketMessage::from_str(&s) {
                Ok(wsm) => {
                    if let Err(err) = self.process_msg(wsm.msg).await {
                        self.logger
                            .error(&format!("Couldn't process message: {}", err))
                    }
                }
                Err(err) => self
                    .logger
                    .error(&format!("While parsing message: {:?}", err)),
            },
            WSMessage::Closed(_) => {}
            WSMessage::Opened(_) => {}
            WSMessage::Error(_) => {}
        }
    }

    /// Processes incoming messages from the signalling server.
    /// This can be either messages requested by this node, or connection
    /// setup requests from another node.
    async fn process_msg(&mut self, msg: WSSignalMessage) -> Result<(), String> {
        match msg {
            WSSignalMessage::Challenge(challenge) => {
                self.logger.info("Processing Challenge message");
                let ma = MessageAnnounce {
                    challenge,
                    node_info: self.node_info.clone(),
                };
                self.send_ws(WSSignalMessage::Announce(ma));
            }
            WSSignalMessage::ListIDsReply(list) => {
                self.logger.info("Processing ListIDsReply message");
                self.update_list(list);
            }
            WSSignalMessage::PeerSetup(pi) => {
                self.logger
                    .info(&format!("Processing PeerSetup message: {}", pi.message));
                let remote = match pi.get_remote(&self.node_info.public) {
                    Some(id) => id,
                    None => {
                        return Err("Got alien PeerSetup".to_string());
                    }
                };
                let remote_clone = remote.clone();
                let rcv = Arc::clone(&self.web_rtc_rcv);
                let log = self.logger.clone();
                let conn = self
                    .connections
                    .entry(remote.clone())
                    .or_insert(NodeConnection::new(
                        Arc::clone(&self.web_rtc),
                        Box::new(move |msg| {
                            if let Ok(rcv_mut) = rcv.try_lock() {
                                rcv_mut(remote_clone.clone(), msg);
                            } else {
                                log.error(
                                    "Couldn't get lock on web_rtc_rcv in process_msg::PeerSetup",
                                );
                            }
                        }),
                        self.logger.clone(),
                    ));

                if let Some(message) = conn
                    .process_peer_setup(pi.message, remote == pi.id_init)
                    .await?
                {
                    self.send_ws(WSSignalMessage::PeerSetup(PeerInfo { message, ..pi }));
                }
            }
            WSSignalMessage::Done => {
                self.logger.info("Processing done message");
            }
            ws => {
                self.logger.info(&format!("Got unusable message: {:?}", ws));
            }
        }
        Ok(())
    }

    /// Sends a websocket message to the signalling server.
    /// This is not a public method, as all communication should happen using
    /// webrtc connections.
    pub fn send_ws(&mut self, msg: WSSignalMessage) {
        self.logger.info(&format!("Sending {} over websocket", msg));
        let wsm = WebSocketMessage { msg };
        if let Err(e) = self.ws.send(wsm.to_string()) {
            self.logger.error(&format!("Error while sending: {:?}", e));
        }
    }

    /// Requests a new node list from the server.
    pub fn update_node_list(&mut self) {
        self.send_ws(WSSignalMessage::ListIDsRequest);
    }

    /// Stores a node list sent from the signalling server.
    fn update_list(&mut self, list: Vec<NodeInfo>) {
        self.list = list
            .iter()
            .filter(|entry| entry.public != self.node_info.public)
            .cloned()
            .collect();
    }

    /// Sends a message to the node dst.
    /// If no connection is setup, the msg will be put in a queue, and
    /// the connection will be setup.
    /// If the connection is in the setup phase, the msg will be put in a queue,
    /// and the method returns.
    /// All messages in the queue will be sent once the connection is set up.
    pub async fn send(&mut self, dst: &U256, msg: String) -> Result<(), String> {
        let dst_clone = dst.clone();
        let rcv = Arc::clone(&self.web_rtc_rcv);
        let log = self.logger.clone();
        let conn = self
            .connections
            .entry(dst.clone())
            .or_insert(NodeConnection::new(
                Arc::clone(&self.web_rtc),
                Box::new(move |msg| {
                    if let Ok(rcv_mut) = rcv.try_lock() {
                        rcv_mut(dst_clone.clone(), msg);
                    } else {
                        log.error("Couldn't get lock on web_rtc_rcv in Intern::send");
                    }
                }),
                self.logger.clone(),
            ));

        let mut message: Option<PeerMessage> = None;
        if let Err(e) = conn.send(msg.clone()) {
            self.logger
                .info(&format!("No connection to {} yet ({}), starting it", dst, e));
            message = conn.process_peer_setup_outgoing(PeerMessage::Init).await?;
            conn.send(msg)?;
        }
        if let Some(message) = message {
            let pi = PeerInfo {
                message,
                id_init: self.node_info.public.clone(),
                id_follow: dst.clone(),
            };
            self.send_ws(WSSignalMessage::PeerSetup(pi));
        }
        Ok(())
    }
}
