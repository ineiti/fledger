use std::error::Error;

use flarch::nodeids::{NodeID, NodeIDs};
use serde::{Deserialize, Serialize};

use super::core::*;

/// These are the messages which will be exchanged between the nodes for this
/// module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModuleMessage {
    Increase(u32),
    Counter(u32),
}

/// First wrap all messages coming into this module and all messages going out in
/// a single message time.
#[derive(Clone, Debug)]
pub enum DHTRoutingMessage {
    Input(DHTRoutingIn),
    Output(DHTRoutingOut),
}

/// The messages here represent all possible interactions with this module.
#[derive(Debug, Clone)]
pub enum DHTRoutingIn {
    FromNetwork(NodeID, ModuleMessage),
    UpdateNodeList(NodeIDs),
}

#[derive(Debug, Clone)]
pub enum DHTRoutingOut {
    ToNetwork(NodeID, ModuleMessage),
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

    /// Processes one generic message and returns either an error
    /// or a Vec<MessageOut>.
    pub fn process_messages(&mut self, msgs: Vec<DHTRoutingIn>) -> Vec<DHTRoutingOut> {
        let mut out = vec![];
        for msg in msgs {
            log::trace!("Got msg: {msg:?}");
            out.extend(match msg {
                DHTRoutingIn::FromNetwork(src, node_msg) => self.process_node_message(src, node_msg),
                DHTRoutingIn::UpdateNodeList(ids) => self.node_list(ids),
            });
        }
        out
    }

    /// Processes a node to node message and returns zero or more
    /// MessageOut.
    pub fn process_node_message(&mut self, _src: NodeID, msg: ModuleMessage) -> Vec<DHTRoutingOut> {
        match msg {
            ModuleMessage::Increase(c) => {
                // When increasing the counter, send 'self' counter to all other nodes.
                // Also send a StorageUpdate message.
                self.core.increase(c);
                return self
                    .nodes
                    .0
                    .iter()
                    .map(|id| {
                        DHTRoutingOut::ToNetwork(
                            id.clone(),
                            ModuleMessage::Counter(self.core.storage.counter),
                        )
                    })
                    .chain(vec![DHTRoutingOut::UpdateStorage(self.core.storage.clone())])
                    .collect();
            }
            ModuleMessage::Counter(c) => log::info!("Got counter from {}: {}", _src, c),
        }
        vec![]
    }

    /// Stores the new node list, excluding the ID of this node.
    fn node_list(&mut self, mut ids: NodeIDs) -> Vec<DHTRoutingOut> {
        self.nodes = ids.remove_missing(&vec![self.our_id].into());
        vec![]
    }
}

/// Convenience method to reduce long lines.
impl From<DHTRoutingIn> for DHTRoutingMessage {
    fn from(msg: DHTRoutingIn) -> Self {
        DHTRoutingMessage::Input(msg)
    }
}

/// Convenience method to reduce long lines.
impl From<DHTRoutingOut> for DHTRoutingMessage {
    fn from(msg: DHTRoutingOut) -> Self {
        DHTRoutingMessage::Output(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() -> Result<(), Box<dyn Error>> {
        let ids = NodeIDs::new(2);
        let id0 = *ids.0.get(0).unwrap();
        let id1 = *ids.0.get(1).unwrap();
        let storage = DHTRoutingStorage::default();
        let mut msg = DHTRoutingMessages::new(storage, DHTRoutingConfig::default(), id0)?;
        msg.process_messages(vec![DHTRoutingIn::UpdateNodeList(ids).into()]);
        let ret =
            msg.process_messages(vec![DHTRoutingIn::FromNetwork(id1, ModuleMessage::Increase(2)).into()]);
        assert_eq!(2, ret.len());
        assert!(matches!(
            ret[0],
            DHTRoutingOut::ToNetwork(_, ModuleMessage::Counter(2))
        ));
        assert!(matches!(
            ret[1],
            DHTRoutingOut::UpdateStorage(DHTRoutingStorage { counter: 2 })
        ));
        Ok(())
    }
}
