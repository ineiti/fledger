use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use ed25519_dalek::Verifier;
use flutils::{
    broker::{Broker, BrokerError, Destination, Subsystem, SubsystemListener},
    nodeids::U256,
};

use crate::{config::NodeInfo, signal::web_rtc::WSSignalMessageToNode};

use super::{
    web_rtc::{MessageAnnounce, NodeStat, PeerInfo, WSSignalMessageFromNode},
    websocket::{WSMessage, WebSocketConnection, WebSocketServer},
};

/// This implements a signalling server. It can be used for tests, in the cli implementation, and
/// will also be used later directly in the network struct to allow for direct node-node setups.
/// It handles the setup phase where the nodes authenticate themselves to the server, and passes
/// PeerInfo messages between nodes.
/// It also handles statistics by forwarding NodeStats to a listener.

#[derive(Clone)]
pub enum Message {
    Input(MessageInput),
    Output(MessageOutput),
    WebSocket(U256, WSMessage),
}

#[derive(Clone)]
pub enum MessageInput {
    NewConnection(Arc<Mutex<Box<dyn WebSocketConnection>>>),
    WebSocket((U256, WSSignalMessageToNode)),
    Timer,
}

#[derive(Clone)]
pub enum MessageOutput {
    NodeStats(Vec<NodeStat>),
    NewNode(U256),
}

pub struct SignalServer {
    connections: HashMap<U256, Arc<Mutex<Box<dyn WebSocketConnection>>>>,
    setup: HashMap<U256, Arc<Mutex<Box<dyn WebSocketConnection>>>>,
    info: HashMap<U256, NodeInfo>,
    ttl: HashMap<U256, u64>,
    ttl_init: u64,
    broker: Broker<Message>,
}

impl SignalServer {
    /// Starts a signal server and returns the broker it uses.
    pub fn start(
        mut ws: Box<dyn WebSocketServer>,
        ttl: u64,
    ) -> Result<Broker<Message>, BrokerError> {
        let sig_serv = SignalServer::new(ttl);
        let mut broker_cl = sig_serv.broker.clone();
        ws.set_cb_connection(Box::new(move |conn| {
            if let Err(e) = broker_cl.emit_msg(Message::Input(MessageInput::NewConnection(
                Arc::new(Mutex::new(conn)),
            ))) {
                log::error!("While sending new connection: {e}");
            }
        }));
        let mut broker_cl = sig_serv.broker.clone();
        broker_cl.add_subsystem(Subsystem::Handler(Box::new(sig_serv)))?;
        Ok(broker_cl)
    }

    /// Creates a new SignalServer.
    pub fn new(ttl_init: u64) -> SignalServer {
        SignalServer {
            connections: HashMap::new(),
            setup: HashMap::new(),
            info: HashMap::new(),
            ttl: HashMap::new(),
            ttl_init,
            broker: Broker::new(),
        }
    }

    fn msg_in(&mut self, msg_in: &MessageInput) -> Vec<MessageOutput> {
        match msg_in {
            MessageInput::NewConnection(conn) => self.msg_in_conn(conn),
            MessageInput::WebSocket((dst, msg)) => {
                self.send_connection(&dst, msg.clone());
            }
            MessageInput::Timer => self.msg_in_timer(),
        }
        vec![]
    }

    fn msg_ws(&mut self, id: &U256, msg: &WSMessage) -> Vec<MessageOutput> {
        self.ttl
            .entry(id.clone())
            .and_modify(|ttl| *ttl = self.ttl_init);
        match msg {
            WSMessage::MessageString(msg_s) => {
                if let Ok(msg_ws) = serde_json::from_str::<WSSignalMessageFromNode>(msg_s) {
                    self.msg_ws_process(id, msg_ws)
                } else {
                    vec![]
                }
            }
            WSMessage::Error(e) => {
                log::error!("While receiving message: {e}");
                self.remove_node(id);
                vec![]
            }
            WSMessage::Closed(e) => {
                log::error!("Closing: {e}");
                self.remove_node(id);
                vec![]
            }
            WSMessage::Opened(_) => vec![],
        }
    }

    fn msg_in_conn(&mut self, conn: &Arc<Mutex<Box<dyn WebSocketConnection>>>) {
        let id = U256::rnd();
        self.setup.insert(id.clone(), conn.clone());
        self.ttl.insert(id.clone(), self.ttl_init);
        let mut broker_cl = self.broker.clone();
        conn.lock().unwrap().set_cb_wsmessage(Box::new(move |msg| {
            if let Err(e) =
                broker_cl.emit_msg_dest(Destination::This, Message::WebSocket(id.clone(), msg))
            {
                log::error!("While sending ws message: {e}");
            }
        }));
        self.send_setup(
            &id,
            serde_json::to_string(&WSSignalMessageToNode::Challenge(2u64, id)).unwrap(),
        );
    }

    fn msg_in_timer(&mut self) {
        let mut to_remove = Vec::new();
        for (id, ttl) in self.ttl.iter_mut() {
            *ttl -= 1;
            if *ttl == 0 {
                to_remove.push(id.clone());
            }
        }
        for id in to_remove {
            self.remove_node(&id);
        }
    }

    // The id is the challange until the announcement succeeds. Then ws_announce calls
    // set_cb_message again to create a new callback using the node-id as id.
    fn msg_ws_process(&mut self, id: &U256, msg: WSSignalMessageFromNode) -> Vec<MessageOutput> {
        match msg {
            WSSignalMessageFromNode::Announce(ann) => self.ws_announce(id, ann),
            WSSignalMessageFromNode::ListIDsRequest => self.ws_list_ids(id),
            WSSignalMessageFromNode::ClearNodes => self.ws_clear(),
            WSSignalMessageFromNode::PeerSetup(pi) => self.ws_peer_setup(id, pi),
            WSSignalMessageFromNode::NodeStats(ns) => self.ws_node_stats(ns),
        }
    }

    fn ws_announce(&mut self, id: &U256, msg: MessageAnnounce) -> Vec<MessageOutput> {
        if let Err(e) = msg.node_info.pubkey.verify(&id.to_bytes(), &msg.signature) {
            log::warn!("Got node with wrong signature: {:?}", e);
            return vec![];
        }
        if let Some(conn) = self.setup.remove(id) {
            self.connections
                .insert(msg.node_info.get_id(), conn.clone());
            let mut broker_cl = self.broker.clone();
            let node_id = msg.node_info.get_id();
            conn.lock().unwrap().set_cb_wsmessage(Box::new(move |msg| {
                if let Err(e) =
                    broker_cl.emit_msg_dest(Destination::This, Message::WebSocket(node_id, msg))
                {
                    log::error!("While sending ws message: {e}");
                }
            }));
        } else {
            log::warn!("Got announcement from a non-setup node: {id}");
            return vec![];
        }

        log::info!("Got announcement from node: {}", msg.node_info.info);
        self.info.insert(msg.node_info.get_id(), msg.node_info);
        vec![MessageOutput::NewNode(*id)]
    }

    fn ws_list_ids(&mut self, id: &U256) -> Vec<MessageOutput> {
        self.send_connection(
            id,
            WSSignalMessageToNode::ListIDsReply(self.info.values().cloned().collect()),
        );
        vec![]
    }

    fn ws_clear(&mut self) -> Vec<MessageOutput> {
        self.setup.clear();
        self.connections.clear();
        self.info.clear();
        vec![]
    }

    fn ws_peer_setup(&mut self, id: &U256, pi: PeerInfo) -> Vec<MessageOutput> {
        if let Some(dst) = pi.get_remote(id) {
            self.send_connection(&dst, WSSignalMessageToNode::PeerSetup(pi));
        }
        vec![]
    }

    fn ws_node_stats(&mut self, ns: Vec<NodeStat>) -> Vec<MessageOutput> {
        vec![MessageOutput::NodeStats(ns)]
    }

    fn send_setup(&self, id: &U256, msg: String) {
        if let Some(conn) = self.setup.get(id) {
            if let Ok(mut conn) = conn.lock() {
                if let Err(e) = conn.send(msg) {
                    log::error!("While sending setup: {e}");
                }
            }
        }
    }

    fn send_connection(&self, id: &U256, msg: WSSignalMessageToNode) {
        if let Some(conn) = self.setup.get(id) {
            if let Ok(mut conn) = conn.lock() {
                if let Err(e) = conn.send(serde_json::to_string(&msg).unwrap()) {
                    log::error!("While sending setup: {e}");
                }
            }
        }
    }

    fn remove_node(&mut self, id: &U256) {
        self.ttl.remove(id);
        self.setup.remove(id);
        self.connections.remove(id);
    }
}

impl SubsystemListener<Message> for SignalServer {
    fn messages(&mut self, from_broker: Vec<&Message>) -> Vec<(Destination, Message)> {
        from_broker
            .iter()
            .flat_map(|msg| match msg {
                Message::Input(msg_in) => self.msg_in(msg_in),
                Message::WebSocket(id, msg) => self.msg_ws(id, msg),
                _ => vec![],
            })
            .map(|msg| (Destination::Others, Message::Output(msg)))
            .collect()
    }
}

#[cfg(test)]
mod test {
    use crate::signal::dummy::WebSocketSimul;

    use super::*;

    #[test]
    fn test_signal_server() -> Result<(), BrokerError> {
        let mut wss = WebSocketSimul::new();
        let ws = wss.new_server();
        let server = SignalServer::start(Box::new(ws), 2)?;
        let i = wss.new_incoming_connection();

        Ok(())
    }
}
