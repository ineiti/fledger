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
    Ping,
    Pong,
    /// A broadcast message to all connected nodes.
    /// Carries the originator of the broadcast message.
    Broadcast(NodeID, NetworkWrapper),
    /// Sends the message to the closest node and emits messages
    /// on the path.
    Closest(NodeID, U256, NetworkWrapper),
    /// Sends the message to a specific node, using Kademlia routing.
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

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub nodes: Vec<NodeID>,
    pub active: usize,
}

/// The message handling part, but only for DHTRouter messages.
#[derive(Debug)]
pub(super) struct Messages {
    core: Kademlia,
    tx: Option<watch::Sender<Stats>>,
}

impl Messages {
    /// Returns a new routing module.
    pub(super) fn new(root: NodeID, cfg: Config) -> (Self, watch::Receiver<Stats>) {
        let (tx, rx) = watch::channel(Stats::default());
        (
            Self {
                core: Kademlia::new(root, cfg),
                tx: Some(tx),
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
                ModuleMessage::Broadcast(u256, network_wrapper) => {
                    vec![DHTRouterOut::MessageBroadcast(u256, network_wrapper).into()]
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

    // Stores the new node list, excluding the ID of this node.
    fn node_list(&mut self, infos: Vec<NodeInfo>) -> Vec<InternOut> {
        self.core
            .add_nodes(infos.iter().map(|i| i.get_id()).collect());
        vec![InternOut::DHTRouter(DHTRouterOut::NodeList(
            self.core.active_nodes(),
        ))]
    }

    // One second passes - and return messages for nodes to ping.
    fn tick(&mut self) -> Vec<InternOut> {
        self.core
            .tick()
            .ping
            .iter()
            .map(|&id| ModuleMessage::Ping.wrapper_network(id))
            .collect()
    }

    fn new_closest(&self, key: U256, msg: NetworkWrapper) -> Vec<InternOut> {
        if let Some(&next_hop) = self.core.route_closest(&key, None).first() {
            vec![ModuleMessage::Closest(self.core.root, key, msg.clone()).wrapper_network(next_hop)]
        } else {
            // log::trace!(
            //     "{}: key {key} is already at its closest node",
            //     self.core.root
            // );
            vec![]
        }
    }

    fn new_direct(&self, dst: NodeID, msg: NetworkWrapper) -> Vec<InternOut> {
        if let Some(&next_hop) = self.core.route_closest(&dst, None).first() {
            vec![ModuleMessage::Direct(self.core.root, dst, msg.clone()).wrapper_network(next_hop)]
        } else {
            // log::trace!(
            //     "{}: couldn't send new request because no hop to {dst} available",
            //     self.core.root
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
        match self.core.route_closest(&key, Some(&last_hop)).first() {
            Some(&next_hop) => vec![
                ModuleMessage::Closest(orig, key, msg.clone()).wrapper_network(next_hop),
                DHTRouterOut::MessageRouting(orig, last_hop, next_hop, key, msg).into(),
            ],
            None => {
                if key == self.core.root {
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
        if dst == self.core.root {
            return vec![DHTRouterOut::MessageDest(orig, last, msg).into()];
        }
        let next_hops = self.core.route_direct(&dst);
        next_hops
            .choose(&mut rand::thread_rng())
            .map(|next_hop| vec![ModuleMessage::Direct(orig, dst, msg).wrapper_network(*next_hop)])
            .unwrap_or(vec![])
    }

    fn update_stats(&mut self) {
        self.tx.clone().map(|tx| {
            tx.send(Stats {
                nodes: self
                    .core
                    .active_nodes()
                    .iter()
                    .chain(self.core.cache_nodes().iter())
                    .cloned()
                    .collect::<Vec<_>>(),
                active: self.core.active_nodes().len(),
            })
            .is_err()
            .then(|| self.tx = None)
        });
    }
}

#[platform_async_trait()]
impl SubsystemHandler<InternIn, InternOut> for Messages {
    async fn messages(&mut self, msgs: Vec<InternIn>) -> Vec<InternOut> {
        let id = self.core.root.clone();
        let out = msgs
            .into_iter()
            // .inspect(|msg| log::debug!("{_id}: DHTRouterIn: {msg:?}"))
            .flat_map(|msg| match msg {
                InternIn::Tick => self.tick(),
                InternIn::DHTRouter(DHTRouterIn::MessageBroadcast(msg)) => self
                    .core
                    .active_nodes()
                    .iter()
                    .map(|dst| {
                        ModuleMessage::Broadcast(id, msg.clone()).wrapper_network(dst.clone())
                    })
                    .collect(),
                InternIn::DHTRouter(DHTRouterIn::MessageClosest(key, msg)) => {
                    self.new_closest(key, msg)
                }
                InternIn::DHTRouter(DHTRouterIn::MessageDirect(key, msg)) => {
                    self.new_direct(key, msg)
                }
                InternIn::Network(from_router) => match from_router {
                    RouterOut::NodeInfoAvailable(node_infos) => self.node_list(node_infos),
                    RouterOut::NetworkWrapperFromNetwork(from, network_wrapper) => {
                        self.process_node_message(from, network_wrapper)
                    }
                    _ => vec![],
                },
            })
            // .inspect(|msg| log::debug!("{id}: Out: {msg:?}"))
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

        let mut msg = Messages::new(
            root,
            Config {
                k: 1,
                ping_interval: 2,
                ping_timeout: 4,
            },
        );

        let out = msg
            .0
            .messages(vec![
                InternIn::Network(RouterOut::NodeInfoAvailable(infos)),
                InternIn::Tick,
            ])
            .await;
        assert_eq!(5, out.len());

        Ok(())
    }
}
