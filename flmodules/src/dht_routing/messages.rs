use std::error::Error;

use flarch::{
    broker_io::SubsystemHandler,
    nodeids::{NodeID, NodeIDs, U256},
    platform_async_trait,
};
use serde::{Deserialize, Serialize};

use crate::overlay::messages::NetworkWrapper;

use super::core::*;

/// These are the messages which will be exchanged between the nodes for this
/// module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModuleMessage {
    Ping,
    Pong,
    // Send the NetworkWrapper message to the closest node of U256.
    // Usually the destination doesn't exist, so whenever there is no
    // closer node to send the message to, propagation stops.
    Route(U256, NetworkWrapper),
}

/// The messages here represent all possible interactions with this module.
#[derive(Debug, Clone)]
pub enum DHTRoutingIn {
    FromNetwork(NodeID, ModuleMessage),
    ToDHTNetwork(NodeID, NetworkWrapper),
    UpdateNodeList(NodeIDs),
    Tick,
}

#[derive(Debug, Clone)]
pub enum DHTRoutingOut {
    ToNetwork(NodeID, ModuleMessage),
    FromDHTNetwork(NodeID, NetworkWrapper),
    UpdateDHTNodeList(NodeIDs),
    UpdateStorage(DHTRoutingStorage),
}

/// The message handling part, but only for DHTRouting messages.
#[derive(Debug)]
pub struct DHTRoutingMessages {
    pub core: DHTRoutingCore,
    nodes: NodeIDs,
    our_id: NodeID,
}

impl DHTRoutingMessages {
    /// Returns a new chat module.
    pub fn new(
        storage: DHTRoutingStorage,
        cfg: DHTRoutingConfig,
        our_id: NodeID,
    ) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            core: DHTRoutingCore::new(storage, cfg),
            nodes: NodeIDs::empty(),
            our_id,
        })
    }

    /// Processes a node to node message and returns zero or more
    /// MessageOut.
    pub fn process_node_message(&mut self, _src: NodeID, msg: ModuleMessage) -> Vec<DHTRoutingOut> {
        match msg {
            ModuleMessage::Ping => todo!(),
            ModuleMessage::Pong => todo!(),
            ModuleMessage::Route(_dst, _network_wrapper) => todo!(),
        }
        // vec![]
    }

    /// Stores the new node list, excluding the ID of this node.
    fn node_list(&mut self, mut ids: NodeIDs) -> Vec<DHTRoutingOut> {
        self.nodes = ids.remove_missing(&vec![self.our_id].into());
        vec![]
    }

    /// One second passes - and return messages for nodes to ping.
    fn tick(&mut self) -> Vec<DHTRoutingOut> {
        let mut out = vec![];
        self.core.tick();
        for id in self.core.nodes_ping() {
            out.push(DHTRoutingOut::ToNetwork(id, ModuleMessage::Ping));
        }
        if self.core.nodes_timeout().len() > 0 {
            out.push(DHTRoutingOut::UpdateDHTNodeList(
                self.core.node_ids().into(),
            ));
        }
        out
    }

    fn route_msg(&mut self, dst: U256, msg: NetworkWrapper) -> Vec<DHTRoutingOut> {
        if let Some(id) = self.core.next_hop(dst) {
            return vec![DHTRoutingOut::ToNetwork(id, ModuleMessage::Route(dst, msg))];
        }
        vec![]
    }
}

#[platform_async_trait()]
impl SubsystemHandler<DHTRoutingIn, DHTRoutingOut> for DHTRoutingMessages {
    /// Processes one generic message and returns either an error
    /// or a Vec<MessageOut>.
    async fn messages(&mut self, msgs: Vec<DHTRoutingIn>) -> Vec<DHTRoutingOut> {
        let mut out = vec![];
        for msg in msgs {
            log::trace!("Got msg: {msg:?}");
            out.extend(match msg {
                DHTRoutingIn::FromNetwork(src, node_msg) => {
                    __self.process_node_message(src, node_msg)
                }
                DHTRoutingIn::UpdateNodeList(ids) => __self.node_list(ids),
                DHTRoutingIn::Tick => self.tick(),
                DHTRoutingIn::ToDHTNetwork(u256, network_wrapper) => {
                    self.route_msg(u256, network_wrapper)
                }
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
