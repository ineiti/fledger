use std::collections::HashMap;

use flarch::{
    broker::{SubsystemHandler, TranslateFrom, TranslateInto},
    nodeids::{NodeID, U256},
    platform_async_trait,
};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::{
    nodeconfig::NodeInfo,
    router::messages::{NetworkWrapper, RouterIn, RouterOut},
    timer::TimerMessage,
};

use super::{
    broker::{DHTRouterIn, DHTRouterOut, MODULE_NAME},
    kademlia::*,
};

/// These are the messages which will be exchanged between the nodes for this
/// module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModuleMessage {
    /// Request if a node is alive or not
    Ping,
    /// Answer for a Ping
    Pong,
    /// Request the IDs of all connected nodes
    ConnectedIDsRequest,
    /// Returns all connected IDs,
    ConnectedIDsReply(Vec<NodeID>),
    /// A message going only to direct neighbours.
    Neighbour(NetworkWrapper),
    /// Sends the message to the closest node and emits messages
    /// on the path.
    Closest(NodeID, U256, NetworkWrapper),
    /// Sends the message to a specific node, using Kademlia routing.
    /// TODO: should also emit msgs, e.g., a reply for a request of a Flo
    /// should get an opportunity for cache in all nodes it passes through.
    /// No messages are emitted during routing.
    Direct(NodeID, NodeID, NetworkWrapper),
}

/// The messages here represent all possible interactions with this module.
#[derive(Debug, Clone)]
pub(super) enum InternIn {
    DHTRouter(DHTRouterIn),
    Network(RouterOut),
    Tick,
}

#[derive(Debug, Clone)]
pub(super) enum InternOut {
    DHTRouter(DHTRouterOut),
    Network(RouterIn),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub all_nodes: Vec<NodeID>,
    pub bucket_nodes: Vec<Vec<NodeID>>,
    pub active: usize,
}

/// The message handling part, but only for DHTRouter messages.
#[derive(Debug)]
pub(super) struct Intern {
    core: Kademlia,
    tx: Option<watch::Sender<Stats>>,
    // This is different than core.active, because there can be connections from other
    // modules, or connections from another node.
    connected: Vec<NodeID>,
    infos: HashMap<NodeID, NodeInfo>,
}

impl Intern {
    /// Returns a new routing module.
    pub(super) fn new(cfg: Config) -> (Self, watch::Receiver<Stats>) {
        let (tx, rx) = watch::channel(Stats::default());
        (
            Self {
                core: Kademlia::new(cfg),
                tx: Some(tx),
                connected: vec![],
                infos: HashMap::new(),
            },
            rx,
        )
    }

    // Processes a node to node message and returns zero or more
    // MessageOut.
    fn process_node_message(&mut self, from: NodeID, msg: NetworkWrapper) -> Vec<InternOut> {
        let mut out = match msg.unwrap_yaml(MODULE_NAME) {
            Some(msg) => match msg {
                ModuleMessage::Ping => vec![(ModuleMessage::Pong).wrapper_network(from)],
                ModuleMessage::Pong => vec![],
                ModuleMessage::Closest(orig, key, msg) => {
                    self.message_closest(orig, from, key, msg)
                }
                ModuleMessage::Direct(orig, dst, msg) => self.message_direct(orig, from, dst, msg),
                ModuleMessage::Neighbour(network_wrapper) => {
                    vec![DHTRouterOut::MessageNeighbour(from, network_wrapper).into()]
                }
                ModuleMessage::ConnectedIDsRequest => {
                    vec![ModuleMessage::ConnectedIDsReply(self.core.active_nodes())
                        .wrapper_network(from)]
                }
                ModuleMessage::ConnectedIDsReply(nodes) => {
                    self.core.add_nodes(nodes);
                    vec![]
                }
            },
            None => vec![],
        };
        if self.core.node_active(&from) {
            out.push(InternOut::DHTRouter(DHTRouterOut::NodeList(
                self.core.active_nodes(),
            )));
        }
        out
    }

    fn msg_dht(&mut self, msg: DHTRouterIn) -> Vec<InternOut> {
        match msg {
            DHTRouterIn::MessageBroadcast(msg) => self
                .core
                .active_nodes()
                .iter()
                .map(|dst| ModuleMessage::Neighbour(msg.clone()).wrapper_network(dst.clone()))
                .collect(),
            DHTRouterIn::MessageClosest(key, msg) => self.new_closest(key, msg),
            DHTRouterIn::MessageDirect(key, msg) => self.new_direct(key, msg),
            DHTRouterIn::MessageNeighbour(dst, network_wrapper) => {
                if !self.connected.contains(&dst) {
                    log::warn!(
                        "{} doesn't have a connection to {} anymore to send message {network_wrapper:?}",
                        self.core.config.root,
                        self.get_id_info(&dst)
                    );
                }
                vec![ModuleMessage::Neighbour(network_wrapper).wrapper_network(dst)]
            }
        }
    }

    fn get_id_info(&self, id: &NodeID) -> String {
        if let Some(info) = self.infos.get(id) {
            format!("{id}/{}", info.name)
        } else {
            format!("{id}")
        }
    }

    fn msg_network(&mut self, msg: RouterOut) -> Vec<InternOut> {
        // log::info!("msg_network({msg})");
        match msg {
            RouterOut::NodeInfoAvailable(node_infos) => self.add_node_infos(node_infos),
            RouterOut::NodeIDsConnected(connected) => {
                self.connected = connected.0.clone();
                self.core.add_nodes(connected.0);
            }
            RouterOut::NetworkWrapperFromNetwork(from, network_wrapper) => {
                return self.process_node_message(from, network_wrapper)
            }
            RouterOut::SystemConfig(conf) => {
                return conf
                    .system_realm
                    .map(|rid| vec![InternOut::DHTRouter(DHTRouterOut::SystemRealm(rid))])
                    .unwrap_or(vec![])
            }
            RouterOut::Disconnected(id) => {
                self.core.node_disconnected(&id);
            }
            RouterOut::Connected(id) => {
                self.core.add_node(id);
            }
            _ => {}
        }
        vec![]
    }

    // Stores the new node list, excluding the ID of this node.
    fn add_node_infos(&mut self, infos: Vec<NodeInfo>) {
        for info in &infos {
            self.infos.insert(info.get_id(), info.clone());
        }
        self.core
            .add_nodes(infos.iter().map(|i| i.get_id()).collect());
    }

    // One second passes - and return messages for nodes to ping.
    fn tick(&mut self) -> Vec<InternOut> {
        let ping_delete = self.core.tick();
        // if !ping_delete.deleted.is_empty() {
        //     log::info!(
        //         "{} deleted {} nodes",
        //         self.core.config.root,
        //         ping_delete.deleted.len()
        //     );
        // }
        ping_delete
            .ping
            .iter()
            .map(|&id| ModuleMessage::Ping.wrapper_network(id))
            .chain(
                self.core
                    .active_nodes()
                    .iter()
                    .map(|id| ModuleMessage::ConnectedIDsRequest.wrapper_network(*id)),
            )
            .collect()
    }

    fn closest_or_connected(&self, key: U256, last: Option<&U256>) -> Vec<U256> {
        if self.connected.contains(&key) {
            vec![key]
        } else {
            self.core.route_closest(&key, last)
        }
    }

    fn new_closest(&self, key: U256, msg: NetworkWrapper) -> Vec<InternOut> {
        if let Some(&next_hop) = self.closest_or_connected(key.clone(), None).first() {
            vec![
                ModuleMessage::Closest(self.core.config.root, key, msg.clone())
                    .wrapper_network(next_hop),
            ]
        } else {
            // log::trace!(
            //     "{}: key {key} is already at its closest node",
            //     self.core.config.root
            // );
            vec![]
        }
    }

    fn new_direct(&self, dst: NodeID, msg: NetworkWrapper) -> Vec<InternOut> {
        if let Some(&next_hop) = self.closest_or_connected(dst.clone(), None).first() {
            vec![
                ModuleMessage::Direct(self.core.config.root, dst, msg.clone())
                    .wrapper_network(next_hop),
            ]
        } else {
            // log::trace!(
            //     "{}: couldn't send new request because no hop to {dst} available",
            //     self.core.config.root
            // );
            vec![]
        }
    }

    fn message_closest(
        &self,
        orig: NodeID,
        last_hop: NodeID,
        key: U256,
        msg: NetworkWrapper,
    ) -> Vec<InternOut> {
        match self
            .closest_or_connected(key.clone(), Some(&last_hop))
            .first()
        {
            Some(&next_hop) => vec![
                ModuleMessage::Closest(orig, key, msg.clone()).wrapper_network(next_hop),
                DHTRouterOut::MessageRouting(orig, last_hop, next_hop, key, msg).into(),
            ],
            None => {
                if key == self.core.config.root {
                    vec![DHTRouterOut::MessageDest(orig, last_hop, msg).into()]
                } else {
                    vec![DHTRouterOut::MessageClosest(orig, last_hop, key, msg).into()]
                }
            }
        }
    }

    fn message_direct(
        &self,
        orig: NodeID,
        last: NodeID,
        dst: NodeID,
        msg: NetworkWrapper,
    ) -> Vec<InternOut> {
        if dst == self.core.config.root {
            return vec![DHTRouterOut::MessageDest(orig, last, msg).into()];
        }
        let next_hops = self.closest_or_connected(dst, Some(&last));
        if next_hops.len() == 0 {
            // log::debug!("{}: cannot hop to {}", self.core.config.root, dst);
            vec![]
        } else {
            next_hops
                .choose(&mut rand::thread_rng())
                .map(|next_hop| {
                    vec![ModuleMessage::Direct(orig, dst, msg).wrapper_network(*next_hop)]
                })
                .unwrap_or(vec![])
        }
    }

    fn update_stats(&mut self) {
        self.tx.clone().map(|tx| {
            tx.send(Stats {
                all_nodes: self
                    .core
                    .active_nodes()
                    .iter()
                    .chain(self.core.cache_nodes().iter())
                    .cloned()
                    .collect::<Vec<_>>(),
                bucket_nodes: self.core.bucket_nodes(),
                active: self.core.active_nodes().len(),
            })
            .is_err()
            .then(|| self.tx = None)
        });
    }
}

#[platform_async_trait()]
impl SubsystemHandler<InternIn, InternOut> for Intern {
    async fn messages(&mut self, msgs: Vec<InternIn>) -> Vec<InternOut> {
        let _id = self.core.config.root.clone();
        let out = msgs
            .into_iter()
            .inspect(|msg| log::trace!("{_id}: DHTRouterIn: {msg:?}"))
            .flat_map(|msg| match msg {
                InternIn::Tick => self.tick(),
                InternIn::DHTRouter(dht_msg) => self.msg_dht(dht_msg),
                InternIn::Network(net_msg) => self.msg_network(net_msg),
            })
            .inspect(|msg| log::trace!("{_id}: DHTRouterOut: {msg:?}"))
            .collect();
        self.update_stats();
        out
    }
}

impl ModuleMessage {
    pub(super) fn wrapper_network(&self, dst: NodeID) -> InternOut {
        InternOut::Network(RouterIn::NetworkWrapperToNetwork(
            dst,
            NetworkWrapper::wrap_yaml(MODULE_NAME, self).unwrap(),
        ))
    }

    fn _from_wrapper(msg: NetworkWrapper) -> Option<ModuleMessage> {
        msg.unwrap_yaml(MODULE_NAME)
    }
}

impl From<DHTRouterOut> for InternOut {
    fn from(value: DHTRouterOut) -> Self {
        InternOut::DHTRouter(value)
    }
}

impl TranslateFrom<TimerMessage> for InternIn {
    fn translate(msg: TimerMessage) -> Option<Self> {
        (msg == TimerMessage::Second).then(|| InternIn::Tick)
    }
}

impl TranslateInto<TimerMessage> for InternOut {
    fn translate(self) -> Option<TimerMessage> {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use flarch::{nodeids::U256, start_logging_filter_level};

    use super::*;

    const LOG_LVL: log::LevelFilter = log::LevelFilter::Info;

    #[tokio::test]
    async fn test_depth() -> anyhow::Result<()> {
        start_logging_filter_level(vec![], LOG_LVL);

        let root = U256::from_str("00").unwrap();
        let node1 = U256::from_str("80").unwrap();
        let node2 = U256::from_str("81").unwrap();
        let node3 = U256::from_str("40").unwrap();
        let node4 = U256::from_str("41").unwrap();

        let infos: Vec<NodeInfo> = [node1, node2, node3, node4]
            .iter()
            .map(|&id| NodeInfo::new_from_id(id))
            .collect();

        let mut msg = Intern::new(Config {
            root,
            k: 1,
            ping_interval: 2,
            ping_timeout: 4,
        });

        let out = msg
            .0
            .messages(vec![
                InternIn::Network(RouterOut::NodeInfoAvailable(infos)),
                InternIn::Tick,
            ])
            .await;
        assert_eq!(4, out.len());

        Ok(())
    }
}
