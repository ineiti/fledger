use bimap::BiMap;
use std::sync::Mutex;
use std::{
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
};
use futures::executor;

use common::{
    node::{
        config::NodeInfo,
        ext_interface::Logger,
        types::U256,
    },
    signal::{
        web_rtc::{WSSignalMessage,WebSocketMessage},
        websocket::WSMessage,
    },
};

use super::node_entry::NodeEntry;

pub struct Internal {
    pub logger: Box<dyn Logger>,
    pub nodes: HashMap<U256, NodeEntry>,
    pub_chal: BiMap<U256, U256>,
}

impl Internal {
    pub fn new(logger: Box<dyn Logger>) -> Arc<Mutex<Internal>> {
        let int = Arc::new(Mutex::new(Internal {
            logger,
            nodes: HashMap::new(),
            pub_chal: BiMap::new(),
        }));
        int
    }

    /// Treats incoming messages from nodes.
    pub fn cb_msg(&mut self, chal: &U256, msg: WSMessage) {
        let src = match self.chal_to_pub(chal) {
            Some(public) => public,
            None => chal.clone(),
        };
        self.logger.info(&format!(
            "Got new message from {:?} -> {:?}: {:?}",
            chal, src, msg
        ));
        match msg {
            WSMessage::MessageString(s) => self.receive_msg(&src, s),
            WSMessage::Closed(_) => self.close_ws(),
            WSMessage::Opened(_) => self.opened_ws(),
            WSMessage::Error(_) => self.error_ws(),
        }
    }

    fn error_ws(&self) {}
    fn close_ws(&self) {}
    fn opened_ws(&self) {}

    fn pub_to_chal(&self, public: &U256) -> Option<U256> {
        match self.pub_chal.get_by_left(public) {
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

    fn receive_msg(&mut self, src: &U256, msg: String) {
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

        match msg_ws.msg {
            // Node sends his information to the server
            WSSignalMessage::Announce(msg_ann) => {
                self.logger
                    .info(&format!("Storing node {:?}", msg_ann.node_info));
                let public = msg_ann.node_info.public.clone();
                self.nodes.retain(|_, ni| {
                    if let Some(info) = ni.info.clone() {
                        return info.public != public;
                    }
                    return true;
                });
                self.pub_chal
                    .insert(msg_ann.node_info.public.clone(), src.clone());
                self.logger
                    .info(&format!("Converter list is {:?}", self.pub_chal));
                self.nodes
                    .entry(src.clone())
                    .and_modify(|ne| ne.info = Some(msg_ann.node_info));
                self.logger.info(&format!("Final list is {:?}", self.nodes));
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
                self.logger.info("Sending list IDs");
                let ids: Vec<NodeInfo> = self
                    .nodes
                    .iter()
                    .filter(|ne| ne.1.info.is_some())
                    .map(|ne| ne.1.info.clone().unwrap())
                    .collect();
                self.send_message_errlog(src, WSSignalMessage::ListIDsReply(ids));
            }

            // Node sends a PeerRequest with some of the data set to 'Some'.
            WSSignalMessage::PeerSetup(pr) => {
                self.logger.info(&format!("Got a PeerSetup {:?}", pr));
                let dst = if *src == pr.id_init {
                    &pr.id_follow
                } else if *src == pr.id_follow {
                    &pr.id_init
                } else {
                    self.logger
                        .error("Node sent a PeerSetup without including itself");
                    return;
                };
                self.send_message_errlog(&dst, WSSignalMessage::PeerSetup(pr.clone()));
            }
            _ => {}
        }
    }

    fn send_message_errlog(&mut self, public: &U256, msg: WSSignalMessage) {
        self.logger
            .info(&format!("Sending to {}: {:?}", public, msg));
        if let Err(e) = self.send_message(public, msg.clone()) {
            self.logger
                .error(&format!("Error {} while sending {:?}", e, msg));
        }
    }

    /// Tries to send a message to the indicated node.
    /// If the node is not reachable, an error will be returned.
    pub fn send_message(&mut self, public: &U256, msg: WSSignalMessage) -> Result<(), String> {
        let msg_str = serde_json::to_string(&WebSocketMessage { msg }).unwrap();
        if let Some(chal) = self.pub_to_chal(public) {
            match self.nodes.entry(chal.clone()) {
                Entry::Occupied(mut e) => {
                    self.logger.info("Internal::send_message executor");
                    executor::block_on((e.get_mut().conn).send(msg_str)).unwrap();
                    self.logger.info("Internal::send_message executor done");
                    Ok(())
                }
                Entry::Vacant(_) => Err("Destination not reachable".to_string()),
            }
        } else {
            Err("Don't know this challenge".to_string())
        }
    }
}
