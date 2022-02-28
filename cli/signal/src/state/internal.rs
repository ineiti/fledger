use ed25519_dalek::Verifier;

use bimap::BiMap;
use csv::Writer;
use log::{debug, error, info, warn};
use std::{
    collections::{hash_map::Entry, HashMap},
    fs::{File, OpenOptions},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use thiserror::Error;

use flnet::{
    config::NodeInfo,
    signal::{
        web_rtc::{NodeStat, WSSignalMessageFromNode, WSSignalMessageToNode},
        websocket::WSMessage,
    },
};
use flutils::nodeids::U256;

use dipstick::*;

use super::node_entry::NodeEntry;
use crate::config::Config;

#[derive(Debug, Error)]
pub enum ISError {
    #[error("During file access: {0}")]
    File(String),
    #[error("Destination not reachable")]
    Unreachable,
    #[error("Unknown challenge")]
    UnknownChallenge,
}

pub struct Internal {
    pub nodes: HashMap<U256, NodeEntry>,
    pub stats: HashMap<U256, Vec<Vec<NodeStat>>>,
    // Left: id - Right: challenge
    pub_chal: BiMap<U256, U256>,
    config: Config,
    graphite: Option<GraphiteScope>,
    file_stats: Option<Writer<File>>,
    file_nodes: Option<Writer<File>>,
}

impl Internal {
    pub fn new(config: Config) -> Result<Arc<Mutex<Internal>>, ISError> {
        let mut graphite = None;
        if let Some(url) = config.graphite_host_port.as_ref() {
            if let Some(path) = config.graphite_path.as_ref() {
                graphite = Some(
                    Graphite::send_to(url)
                        .expect("Connected")
                        .named(path)
                        .metrics(),
                );
            }
        }
        let int = Arc::new(Mutex::new(Internal {
            stats: HashMap::new(),
            nodes: HashMap::new(),
            pub_chal: BiMap::new(),
            config: config.clone(),
            graphite,
            file_stats: match config.file_stats {
                Some(name) => Some(csv::Writer::from_writer(
                    OpenOptions::new()
                        .append(true)
                        .create(true)
                        .open(name)
                        .map_err(|e| ISError::File(e.to_string()))?,
                )),
                None => None,
            },
            file_nodes: match config.file_nodes {
                Some(name) => Some(csv::Writer::from_writer(
                    OpenOptions::new()
                        .append(true)
                        .create(true)
                        .open(name)
                        .map_err(|e| ISError::File(e.to_string()))?,
                )),
                None => None,
            },
        }));
        Ok(int)
    }

    /// Treats incoming messages from nodes.
    pub fn cb_msg(&mut self, chal: &U256, msg: WSMessage) {
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
        self.pub_chal.get_by_left(id).copied()
    }

    fn chal_to_pub(&self, chal: &U256) -> Option<U256> {
        self.pub_chal.get_by_right(chal).copied()
    }

    /// Receives a message from the websocket. Src is the challenge-ID, which is
    /// random and only tied to the id ID through self.pub_chal.
    fn receive_msg(&mut self, chal: &U256, msg: String) {
        let msg_ws = match serde_json::from_str::<WSSignalMessageFromNode>(&msg) {
            Ok(mw) => mw,
            Err(e) => {
                error!("Couldn't parse message as WebSocketMessage: {:?}", e);
                return;
            }
        };

        if let Some(node) = self.nodes.get_mut(chal) {
            node.last_seen = Instant::now();
        }

        match msg_ws {
            // Node sends his information to the server
            WSSignalMessageFromNode::Announce(msg_ann) => {
                if let Err(e) = msg_ann
                    .node_info
                    .pubkey
                    .verify(&chal.to_bytes(), &msg_ann.signature)
                {
                    warn!("Got node with wrong signature: {:?}", e);
                    return;
                }
                info!("Storing node {:?}", msg_ann.node_info);
                let id = msg_ann.node_info.get_id();
                self.nodes.retain(|_, ni| {
                    if let Some(info) = ni.info.clone() {
                        return info.get_id() != id;
                    }
                    true
                });
                if let Some(f) = self.file_nodes.as_mut() {
                    if let Ok(s) = serde_json::to_string(&msg_ann.node_info) {
                        info!("Serializing node {}", s);
                    }
                    let ni = msg_ann.node_info.clone();
                    if let Err(e) = f.write_record(&[ni.get_id().to_string(), ni.info, ni.client]) {
                        error!("While serializing node: {}", e);
                    } else if let Err(e) = f.flush() {
                        error!("While flushing csv: {}", e);
                    }
                }
                self.pub_chal.insert(msg_ann.node_info.get_id(), *chal);
                self.nodes
                    .entry(*chal)
                    .and_modify(|ne| ne.info = Some(msg_ann.node_info));
            }

            // Node requests a list of all currently connected nodes,
            // including itself.
            WSSignalMessageFromNode::ListIDsRequest => {
                let ids: Vec<NodeInfo> = self
                    .nodes
                    .iter()
                    .filter(|ne| ne.1.info.is_some())
                    .map(|ne| ne.1.info.clone().unwrap())
                    .collect();
                if let Some(src) = self.chal_to_pub(chal) {
                    self.send_message_errlog(&src, WSSignalMessageToNode::ListIDsReply(ids));
                }
            }

            // Node sends a PeerRequest with some of the data set to 'Some'.
            WSSignalMessageFromNode::PeerSetup(pr) => {
                info!("Got a PeerSetup {}", pr);
                if let Some(src) = self.chal_to_pub(chal) {
                    let dst = if src == pr.id_init {
                        &pr.id_follow
                    } else if src == pr.id_follow {
                        &pr.id_init
                    } else {
                        error!("Node sent a PeerSetup without including itself");
                        return;
                    };
                    self.send_message_errlog(dst, WSSignalMessageToNode::PeerSetup(pr.clone()));
                } else {
                    error!("Got a PeerSetup for an unknown node");
                }
            }

            WSSignalMessageFromNode::NodeStats(ns) => {
                if let Some(f) = self.file_stats.as_mut() {
                    info!("Writing stats");
                    for n in ns.iter() {
                        let node_id = match self.nodes.get(chal) {
                            Some(ne) => match ne.info.as_ref() {
                                Some(info) => info.get_id(),
                                None => U256::rnd(),
                            },
                            None => U256::rnd(),
                        };

                        info!("Stats {:?}", &n);
                        if let Err(e) = f.write_record(&[
                            node_id.to_string(),
                            n.id.to_string(),
                            n.version.clone(),
                            n.ping_rx.to_string(),
                            n.ping_ms.to_string(),
                        ]) {
                            error!("Couldn't serialize stats: {}", e);
                        } else if let Err(e) = f.flush() {
                            error!("While flushing csv: {}", e);
                        }
                    }
                }
                if let Some(node) = self.nodes.get(chal) {
                    if node.info.is_some() {
                        info!(
                            "Got node statistics from '{}' about {} nodes",
                            node.info.as_ref().unwrap().info,
                            ns.len()
                        );
                    }
                    if let Some(src) = self.chal_to_pub(chal) {
                        if let Some(gs) = self.graphite.as_ref() {
                            gs.counter("pings").count(ns.len());
                        }
                        self.stats.entry(src).or_insert_with(Vec::new);
                        self.stats.entry(src).and_modify(|e| e.push(ns));
                    } else {
                        warn!("Couldn't get node-id for challenge {}", chal);
                    }
                }
            }

            _ => {}
        }
    }

    fn send_message_errlog(&mut self, id: &U256, msg: WSSignalMessageToNode) {
        debug!("Sending to {}: {:?}", id, msg);
        if let Err(e) = self.send_message(id, msg.clone()) {
            error!("Error {} while sending {:?}", e, msg);
        }
    }

    /// Tries to send a message to the indicated node.
    /// If the node is not reachable, an error will be returned.
    pub fn send_message(&mut self, id: &U256, msg: WSSignalMessageToNode) -> Result<(), ISError> {
        let msg_str = serde_json::to_string(&msg).unwrap();
        if let Some(chal) = self.pub_to_chal(id) {
            match self.nodes.entry(chal) {
                Entry::Occupied(mut e) => {
                    if let Err(e) = (e.get_mut().conn).send(msg_str) {
                        error!("Couldn't send message: {}", e);
                    }
                    Ok(())
                }
                Entry::Vacant(_) => Err(ISError::Unreachable),
            }
        } else {
            Err(ISError::UnknownChallenge)
        }
    }

    /// Removes all nodes that haven't sent anything for a given delay.
    pub fn cleanup(&mut self) {
        let delay = Duration::from_millis(self.config.cleanup_interval * 1000);
        let now = Instant::now();
        let filtered: Vec<U256> = self
            .nodes
            .iter()
            .filter(|(_k, node)| now.duration_since(node.last_seen) > delay)
            .map(|(k, _v)| *k)
            .collect();
        for key in filtered.iter() {
            info!("Removing node {}", key);
            self.nodes.remove(key);
            self.pub_chal.remove_by_right(key);
        }
    }
}
