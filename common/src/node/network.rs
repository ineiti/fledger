use crate::node::{config::NodeInfo, ext_interface::Logger, types::U256};
use crate::signal::{
    web_rtc::{MessageAnnounce, PeerInfo, WSSignalMessage, WebRTCSpawner, WebSocketMessage},
    websocket::{WSMessage, WebSocketConnection},
};

use std::sync::{
    mpsc::{channel, Receiver, Sender},
    Mutex,
};
use std::{collections::HashMap, sync::Arc};

use node_connection::{NCInput, NodeConnection};

use self::node_connection::NCOutput;
mod connection_state;
mod node_connection;

pub enum NOutput {
    WebRTC(U256, String),
}

pub enum NInput {
    WebRTC(U256, String),
}

pub struct Network {
    pub output_rx: Receiver<NOutput>,
    pub input_tx: Sender<NInput>,
    output_tx: Sender<NOutput>,
    input_rx: Receiver<NInput>,
    list: Vec<NodeInfo>,
    ws: Box<dyn WebSocketConnection>,
    ws_rx: Receiver<WSMessage>,
    web_rtc: Arc<Mutex<WebRTCSpawner>>,
    connections: HashMap<U256, NodeConnection>,
    node_info: NodeInfo,
    logger: Box<dyn Logger>,
}

/// Network combines a websocket to connect to the signal server with
/// a WebRTC trait to connect to other nodes.
/// It supports setting up automatic connetions to other nodes.
impl Network {
    pub fn new(
        logger: Box<dyn Logger>,
        node_info: NodeInfo,
        mut ws: Box<dyn WebSocketConnection>,
        web_rtc: WebRTCSpawner,
    ) -> Network {
        let (output_tx, output_rx) = channel::<NOutput>();
        let (input_tx, input_rx) = channel::<NInput>();
        let (ws_tx, ws_rx) = channel::<WSMessage>();
        let log_clone = logger.clone();
        ws.set_cb_wsmessage(Box::new(move |msg| {
            // log_clone.info(&format!("dbg: Got message from websocket: {:?}", msg));
            if let Err(e) = ws_tx.send(msg) {
                log_clone.info(&format!("Couldn't send msg over ws-channel: {}", e));
            }
        }));
        let net = Network {
            list: vec![],
            output_tx,
            output_rx,
            input_tx,
            input_rx,
            ws,
            ws_rx,
            web_rtc: Arc::new(Mutex::new(web_rtc)),
            connections: HashMap::new(),
            node_info,
            logger,
        };
        net
    }

    /// Process all connections with their waiting messages.
    pub async fn process(&mut self) -> Result<(), String> {
        self.process_input().await?;
        self.process_websocket().await?;
        self.process_connections().await?;
        Ok(())
    }

    async fn process_input(&mut self) -> Result<(), String> {
        let msgs: Vec<NInput> = self.input_rx.try_iter().collect();
        for msg in msgs {
            match msg {
                NInput::WebRTC(id, msg) => self.send(&id, msg).await?,
            }
        }
        Ok(())
    }

    async fn process_websocket(&mut self) -> Result<(), String> {
        let msgs: Vec<WSMessage> = self.ws_rx.try_iter().collect();
        for msg in msgs {
            // self.logger.info(&format!("dbg: Network::process_ws({:?})", msg));
            match msg {
                WSMessage::MessageString(s) => {
                    self.process_msg(WebSocketMessage::from_str(&s)?.msg)
                        .await?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn process_connections(&mut self) -> Result<(), String> {
        let mut ws_msgs = vec![];
        let conns: Vec<(&U256, &mut NodeConnection)> = self.connections.iter_mut().collect();
        for conn in conns {
            let outputs: Vec<NCOutput> = conn.1.output_rx.try_iter().collect();
            for output in outputs {
                // self.logger.info(&format!("dbg: Network::process {:?}", output));
                match output {
                    NCOutput::WebSocket(message, remote) => {
                        let (id_init, id_follow) = match remote {
                            true => (conn.0.clone(), self.node_info.public.clone()),
                            false => (self.node_info.public.clone(), conn.0.clone()),
                        };
                        let peer_info = PeerInfo {
                            id_init,
                            id_follow,
                            message,
                        };
                        ws_msgs.push(WSSignalMessage::PeerSetup(peer_info));
                    }
                    NCOutput::WebRTCMessage(msg) => self
                        .output_tx
                        .send(NOutput::WebRTC(conn.0.clone(), msg))
                        .map_err(|e| e.to_string())?,
                }
            }
            conn.1.process().await?;
        }
        for msg in ws_msgs {
            self.ws_send(msg)?;
        }
        Ok(())
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
                self.ws.send(
                    WebSocketMessage {
                        msg: WSSignalMessage::Announce(ma),
                    }
                    .to_string(),
                )?;
                self.update_node_list()?;
            }
            WSSignalMessage::ListIDsReply(list) => {
                self.logger.info("Processing ListIDsReply message");
                self.update_list(list);
            }
            WSSignalMessage::PeerSetup(pi) => {
                // self.logger
                //     .info(&format!("dbg: Network::Processing PeerSetup message: {}", pi.message));
                let remote_node = match pi.get_remote(&self.node_info.public) {
                    Some(id) => id,
                    None => {
                        return Err("Got alien PeerSetup".to_string());
                    }
                };
                let remote = remote_node == pi.id_init;
                let conn = self
                    .connections
                    .entry(remote_node)
                    .or_insert(NodeConnection::new(
                        self.logger.clone(),
                        Arc::clone(&self.web_rtc),
                    )?);
                conn.input_tx
                    .send(NCInput::WebSocket(pi.message, remote))
                    .map_err(|e| e.to_string())?;
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

    /// Requests a new node list from the server.
    pub fn update_node_list(&mut self) -> Result<(), String> {
        self.ws_send(WSSignalMessage::ListIDsRequest)
    }

    /// Stores a node list sent from the signalling server.
    fn update_list(&mut self, list: Vec<NodeInfo>) {
        self.list = list
            .iter()
            .filter(|entry| entry.public != self.node_info.public)
            .cloned()
            .collect();
    }

    fn ws_send(&mut self, msg: WSSignalMessage) -> Result<(), String>{
        self.ws.send(WebSocketMessage{msg}.to_string())
    }

    /// Sends a message to the node dst.
    /// If no connection is active yet, a new one will be created.
    /// NodeConnection will take care of putting the message in a queue while
    /// the setup is finishing.
    async fn send(&mut self, dst: &U256, msg: String) -> Result<(), String> {
        let conn = self
            .connections
            .entry(dst.clone())
            .or_insert(NodeConnection::new(
                self.logger.clone(),
                Arc::clone(&self.web_rtc),
            )?);
        conn.send(msg.clone())
    }

    /// Prints the states of all connections.
    pub async fn print_states(&self) -> Result<(), String> {
        for (_id, conn) in self.connections.iter() {
            for dir in conn.get_stats().await? {
                if let Some(stats) = dir {
                    self.logger.info(&format!("Connection is: {:?}", stats));
                }
            }
        }
        Ok(())
    }

    pub fn clear_nodes(&mut self) -> Result<(), String> {
        self.ws_send(WSSignalMessage::ClearNodes)
    }

    pub fn get_list(&self) -> Vec<NodeInfo> {
        self.list.clone()
    }
}
