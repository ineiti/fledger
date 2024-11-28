use flarch::{
    broker_io::{SubsystemHandler, TranslateFrom, TranslateInto},
    nodeids::{NodeID, U256},
    platform_async_trait,
};
use serde::{Deserialize, Serialize};

use crate::{
    nodeconfig::NodeInfo,
    overlay::messages::{NetworkWrapper, OverlayIn, OverlayOut},
    timer::TimerMessage,
};

use super::{
    broker::{DHTRoutingIn, DHTRoutingOut, MODULE_NAME},
    kademlia::*,
};

/// These are the messages which will be exchanged between the nodes for this
/// module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModuleMessage {
    Ping,
    Pong,
    /// Source, Key (or NodeID), msg
    DHTMessage(NodeID, U256, NetworkWrapper),
}

/// The messages here represent all possible interactions with this module.
#[derive(Debug, Clone)]
pub(super) enum InternIn {
    DHTRouting(DHTRoutingIn),
    Network(OverlayOut),
    Tick,
}

#[derive(Debug, Clone)]
pub(super) enum InternOut {
    DHTRouting(DHTRoutingOut),
    Network(OverlayIn),
}

/// The message handling part, but only for DHTRouting messages.
#[derive(Debug)]
pub struct DHTRoutingMessages {
    pub core: Kademlia,
}

impl DHTRoutingMessages {
    /// Returns a new routing module.
    pub fn new(root: NodeID, cfg: Config) -> Self {
        Self {
            core: Kademlia::new(root, cfg),
        }
    }

    // Processes a node to node message and returns zero or more
    // MessageOut.
    fn process_node_message(&mut self, from: NodeID, msg: NetworkWrapper) -> Vec<InternOut> {
        self.core.node_active(&from);
        match msg.unwrap_yaml(MODULE_NAME) {
            Some(msg) => match msg {
                ModuleMessage::Ping => vec![(ModuleMessage::Pong).wrapper_network(from)],
                ModuleMessage::Pong => vec![],
                ModuleMessage::DHTMessage(orig, key, msg) => self.dht_message(orig, from, key, msg),
            },
            None => vec![],
        }
    }

    // Stores the new node list, excluding the ID of this node.
    fn node_list(&mut self, infos: Vec<NodeInfo>) -> Vec<InternOut> {
        self.core
            .add_nodes(infos.iter().map(|i| i.get_id()).collect());
        vec![]
    }

    // One second passes - and return messages for nodes to ping.
    fn tick(&mut self) -> Vec<InternOut> {
        let mut out = vec![];
        let nodes = self.core.tick();
        for id in nodes.ping {
            out.push(ModuleMessage::Ping.wrapper_network(id));
        }
        out
    }

    fn new_dht_message(&self, key: U256, msg: NetworkWrapper) -> Vec<InternOut> {
        if let Some(&next_hop) = self.core.nearest_nodes(&key).first() {
            vec![ModuleMessage::DHTMessage(self.core.root, key, msg.clone())
                .wrapper_network(next_hop)]
        } else {
            log::warn!("Couldn't send new request because no nodes found!");
            vec![]
        }
    }

    fn dht_message(
        &self,
        orig: NodeID,
        last_hop: NodeID,
        key: U256,
        msg: NetworkWrapper,
    ) -> Vec<InternOut> {
        match self.core.nearest_nodes(&key).first() {
            Some(&next_hop) => vec![
                ModuleMessage::DHTMessage(orig, key, msg.clone()).wrapper_network(next_hop),
                DHTRoutingOut::MessageRouting(orig, last_hop, next_hop, key, msg).into(),
            ],
            None => {
                if key == self.core.root {
                    vec![DHTRoutingOut::MessageDest(orig, last_hop, msg).into()]
                } else {
                    vec![DHTRoutingOut::MessageClosest(orig, last_hop, key, msg).into()]
                }
            }
        }
    }
}

#[platform_async_trait()]
impl SubsystemHandler<InternIn, InternOut> for DHTRoutingMessages {
    /// Processes one generic message and returns either an error
    /// or a Vec<MessageOut>.
    async fn messages(&mut self, msgs: Vec<InternIn>) -> Vec<InternOut> {
        let mut out = vec![];
        for msg in msgs {
            log::trace!("Got msg: {msg:?}");
            out.extend(match msg {
                InternIn::Tick => self.tick(),
                InternIn::DHTRouting(DHTRoutingIn::DHTMessage(key, msg)) => {
                    self.new_dht_message(key, msg)
                }
                InternIn::Network(overlay_out) => match overlay_out {
                    OverlayOut::NodeInfoAvailable(node_infos) => self.node_list(node_infos),
                    OverlayOut::NetworkWrapperFromNetwork(from, network_wrapper) => {
                        self.process_node_message(from, network_wrapper)
                    }
                    _ => vec![],
                },
            });
        }
        out
    }
}

impl ModuleMessage {
    pub(super) fn wrapper_network(&self, dst: NodeID) -> InternOut {
        InternOut::Network(OverlayIn::NetworkWrapperToNetwork(
            dst,
            NetworkWrapper::wrap_yaml(MODULE_NAME, self).unwrap(),
        ))
    }

    fn _from_wrapper(msg: NetworkWrapper) -> Option<ModuleMessage> {
        msg.unwrap_yaml(MODULE_NAME)
    }
}

impl From<DHTRoutingOut> for InternOut {
    fn from(value: DHTRoutingOut) -> Self {
        InternOut::DHTRouting(value)
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
    use std::error::Error;

    // use super::*;

    #[test]
    fn test_something() -> Result<(), Box<dyn Error>> {
        // let ids = NodeIDs::new(2);
        // let id0 = *ids.0.get(0).unwrap();
        // let id1 = *ids.0.get(1).unwrap();
        // let storage = DHTRoutingStorage::default();
        // let mut msg = DHTRoutingMessages::new(storage, DHTRoutingConfig::default(), id0)?;
        // msg.process_messages(vec![DHTRoutingIn::UpdateNodeList(ids).into()]);
        // let ret =
        //     msg.process_messages(vec![DHTRoutingIn::FromNetwork(id1, ModuleMessage::Increase(2)).into()]);
        // assert_eq!(2, ret.len());
        // assert!(matches!(
        //     ret[0],
        //     DHTRoutingOut::ToNetwork(_, ModuleMessage::Counter(2))
        // ));
        // assert!(matches!(
        //     ret[1],
        //     DHTRoutingOut::UpdateStorage(DHTRoutingStorage { counter: 2 })
        // ));
        Ok(())
    }
}
