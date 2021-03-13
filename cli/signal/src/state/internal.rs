use bimap::BiMap;
use futures::executor;
use std::{
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use common::{
    node::{config::NodeInfo, ext_interface::Logger, types::U256},
    signal::{
        web_rtc::{WSSignalMessage, WebSocketMessage, NodeStat},
        websocket::WSMessage,
    },
};

use super::node_entry::NodeEntry;

pub struct Internal {
    pub logger: Box<dyn Logger>,
    pub nodes: HashMap<U256, NodeEntry>,
    pub stats: HashMap<U256, Vec<Vec<NodeStat>>>,
    // Left: id - Right: challenge
    pub_chal: BiMap<U256, U256>,
}

impl Internal {
    pub fn new(logger: Box<dyn Logger>) -> Arc<Mutex<Internal>> {
        let int = Arc::new(Mutex::new(Internal {
            logger,
            stats: HashMap::new(),
            nodes: HashMap::new(),
            pub_chal: BiMap::new(),
        }));
        int
    }

    /// Treats incoming messages from nodes.
    pub fn cb_msg(&mut self, chal: &U256, msg: WSMessage) {
        self.logger.info(&format!("Got message: {:?}", msg));
        match msg {
            WSMessage::MessageString(s) => self.receive_msg(chal, s),
            WSMessage::Closed(_) => self.close_ws(),
            WSMessage::Opened(_) => self.opened_ws(),
            WSMessage::Error(_) => self.error_ws(),
        }
    }

    fn error_ws(&self) {}
    fn close_ws(&self) {}
    fn opened_ws(&self) {}

    fn pub_to_chal(&self, id: &U256) -> Option<U256> {
        match self.pub_chal.get_by_left(id) {
            Some(p) => Some(p.clone()),
            None => None,
        }
    }

    fn chal_to_pub(&self, chal: &U256) -> Option<U256> {
        match self.pub_chal.get_by_right(chal) {
            Some(p) => Some(p.clone()),
            None => None,
        }
    }

    /// Receives a message from the websocket. Src is the challenge-ID, which is
    /// random and only tied to the id ID through self.pub_chal.
    fn receive_msg(&mut self, chal: &U256, msg: String) {
        let msg_ws = match WebSocketMessage::from_str(&msg) {
            Ok(mw) => mw,
            Err(e) => {
                self.logger.error(&format!(
                    "Couldn't parse message as WebSocketMessage: {:?}",
                    e
                ));
                return;
            }
        };

        if let Some(node) = self.nodes.get_mut(chal) {
            node.last_seen = Instant::now();
        }

        match msg_ws.msg {
            // Node sends his information to the server
            WSSignalMessage::Announce(msg_ann) => {
                self.logger
                    .info(&format!("Storing node {:?}", msg_ann.node_info));
                let id = msg_ann.node_info.id.clone();
                self.nodes.retain(|_, ni| {
                    if let Some(info) = ni.info.clone() {
                        return info.id != id;
                    }
                    return true;
                });
                self.pub_chal
                    .insert(msg_ann.node_info.id.clone(), chal.clone());
                self.nodes
                    .entry(chal.clone())
                    .and_modify(|ne| ne.info = Some(msg_ann.node_info));
            }

            // Node requests deleting of the list of all nodes
            // TODO: remove this after debugging is done
            WSSignalMessage::ClearNodes => {
                self.logger.info("Clearing nodes");
                self.nodes.clear();
            }

            // Node requests a list of all currently connected nodes,
            // including itself.
            WSSignalMessage::ListIDsRequest => {
                let ids: Vec<NodeInfo> = self
                    .nodes
                    .iter()
                    .filter(|ne| ne.1.info.is_some())
                    .map(|ne| ne.1.info.clone().unwrap())
                    .collect();
                if let Some(src) = self.chal_to_pub(chal) {
                    self.send_message_errlog(&src, WSSignalMessage::ListIDsReply(ids));
                }
            }

            // Node sends a PeerRequest with some of the data set to 'Some'.
            WSSignalMessage::PeerSetup(pr) => {
                self.logger.info(&format!("Got a PeerSetup {:?}", pr));
                let src = self.chal_to_pub(chal).unwrap();
                let dst = if src == pr.id_init {
                    &pr.id_follow
                } else if src == pr.id_follow {
                    &pr.id_init
                } else {
                    self.logger
                        .error("Node sent a PeerSetup without including itself");
                    return;
                };
                self.send_message_errlog(&dst, WSSignalMessage::PeerSetup(pr.clone()));
            }

            WSSignalMessage::NodeStats(ns) => {
                if let Some(node) = self.nodes.get(&chal) {
                    self.logger.info(&format!(
                        "Got node statistics from '{}' about {} nodes",
                        node.info.as_ref().unwrap().info,
                        ns.len()
                    ));
                    let src = self.chal_to_pub(chal).unwrap();
                    self.stats.entry(src.clone()).or_insert(vec![]);
                    self.stats.entry(src).and_modify(|e| e.push(ns));
                }
            }
            _ => {}
        }
    }

    fn send_message_errlog(&mut self, id: &U256, msg: WSSignalMessage) {
        self.logger.info(&format!("Sending to {}: {:?}", id, msg));
        if let Err(e) = self.send_message(id, msg.clone()) {
            self.logger
                .error(&format!("Error {} while sending {:?}", e, msg));
        }
    }

    /// Tries to send a message to the indicated node.
    /// If the node is not reachable, an error will be returned.
    pub fn send_message(&mut self, id: &U256, msg: WSSignalMessage) -> Result<(), String> {
        let msg_str = serde_json::to_string(&WebSocketMessage { msg }).unwrap();
        if let Some(chal) = self.pub_to_chal(id) {
            match self.nodes.entry(chal.clone()) {
                Entry::Occupied(mut e) => {
                    if let Err(e) = executor::block_on((e.get_mut().conn).send(msg_str)) {
                        self.logger.error(&format!("Couldn't send message: {}", e));
                    }
                    Ok(())
                }
                Entry::Vacant(_) => Err("Destination not reachable".to_string()),
            }
        } else {
            Err("Don't know this challenge".to_string())
        }
    }

    /// Removes all nodes that haven't sent anything for a given delay.
    pub fn cleanup(&mut self, delay: Duration) {
        let now = Instant::now();
        let filtered: Vec<U256> = self
            .nodes
            .iter()
            .filter(|(_k, node)| now.duration_since(node.last_seen) > delay)
            .map(|(k, _v)| k.clone())
            .collect();
        for key in filtered.iter() {
            self.logger.info(&format!("Removing node {}", key));
            self.nodes.remove(key);
            self.pub_chal.remove_by_right(key);
        }
    }
}
