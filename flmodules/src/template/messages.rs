use std::error::Error;

use crate::nodeids::{NodeID, NodeIDs};
use serde::{Deserialize, Serialize};

use super::core::*;

/// First wrap all messages coming into this module and all messages going out in
/// a single message time.
#[derive(Clone, Debug)]
pub enum TemplateMessage {
    Input(TemplateIn),
    Output(TemplateOut),
}

/// The messages here represent all possible interactions with this module.
#[derive(Debug, Clone)]
pub enum TemplateIn {
    Node(NodeID, MessageNode),
    UpdateNodeList(NodeIDs),
}

#[derive(Debug, Clone)]
pub enum TemplateOut {
    Node(NodeID, MessageNode),
    UpdateStorage(TemplateStorage),
}

/// These are the messages which will be exchanged between the nodes for this
/// module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageNode {
    Increase(u32),
    Counter(u32),
}

/// The message handling part, but only for template messages.
#[derive(Debug)]
pub struct TemplateMessages {
    pub core: TemplateCore,
    nodes: NodeIDs,
    our_id: NodeID,
}

impl TemplateMessages {
    /// Returns a new chat module.
    pub fn new(
        storage: TemplateStorage,
        cfg: TemplateConfig,
        our_id: NodeID,
    ) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            core: TemplateCore::new(storage, cfg),
            nodes: NodeIDs::empty(),
            our_id,
        })
    }

    /// Processes one generic message and returns either an error
    /// or a Vec<MessageOut>.
    pub fn process_messages(&mut self, msgs: Vec<TemplateIn>) -> Vec<TemplateOut> {
        let mut out = vec![];
        for msg in msgs {
            log::trace!("Got msg: {msg:?}");
            out.extend(match msg {
                TemplateIn::Node(src, node_msg) => self.process_node_message(src, node_msg),
                TemplateIn::UpdateNodeList(ids) => self.node_list(ids),
            });
        }
        out
    }

    /// Processes a node to node message and returns zero or more
    /// MessageOut.
    pub fn process_node_message(&mut self, _src: NodeID, msg: MessageNode) -> Vec<TemplateOut> {
        match msg {
            MessageNode::Increase(c) => {
                // When increasing the counter, send 'self' counter to all other nodes.
                // Also send a StorageUpdate message.
                self.core.increase(c);
                return self
                    .nodes
                    .0
                    .iter()
                    .map(|id| {
                        TemplateOut::Node(
                            id.clone(),
                            MessageNode::Counter(self.core.storage.counter),
                        )
                    })
                    .chain(vec![TemplateOut::UpdateStorage(self.core.storage.clone())])
                    .collect();
            }
            MessageNode::Counter(c) => log::info!("Got counter from {}: {}", _src, c),
        }
        vec![]
    }

    /// Stores the new node list, excluding the ID of this node.
    fn node_list(&mut self, mut ids: NodeIDs) -> Vec<TemplateOut> {
        self.nodes = ids.remove_missing(&vec![self.our_id].into());
        vec![]
    }
}

/// Convenience method to reduce long lines.
impl From<TemplateIn> for TemplateMessage {
    fn from(msg: TemplateIn) -> Self {
        TemplateMessage::Input(msg)
    }
}

/// Convenience method to reduce long lines.
impl From<TemplateOut> for TemplateMessage {
    fn from(msg: TemplateOut) -> Self {
        TemplateMessage::Output(msg)
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
        let storage = TemplateStorage::default();
        let mut msg = TemplateMessages::new(storage, TemplateConfig::default(), id0)?;
        msg.process_messages(vec![TemplateIn::UpdateNodeList(ids).into()]);
        let ret =
            msg.process_messages(vec![TemplateIn::Node(id1, MessageNode::Increase(2)).into()]);
        assert_eq!(2, ret.len());
        assert!(matches!(
            ret[0],
            TemplateOut::Node(_, MessageNode::Counter(2))
        ));
        assert!(matches!(
            ret[1],
            TemplateOut::UpdateStorage(TemplateStorage { counter: 2 })
        ));
        Ok(())
    }
}
