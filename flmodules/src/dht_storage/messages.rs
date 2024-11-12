use std::error::Error;

use flarch::nodeids::{NodeID, NodeIDs};
use serde::{Deserialize, Serialize};

use crate::flo::dht::{DHTFlo, DHTStorageConfig};

use super::core::*;

/// First wrap all messages coming into this module and all messages going out in
/// a single message time.
#[derive(Clone, Debug)]
pub enum DHTStorageMessage {
    Input(DHTStorageIn),
    Output(DHTStorageOut),
}

/// The messages here represent all possible interactions with this module.
#[derive(Debug, Clone)]
pub enum DHTStorageIn {
    Node(NodeID, MessageNode),
    UpdateNodeList(NodeIDs),
}

#[derive(Debug, Clone)]
pub enum DHTStorageOut {
    Node(NodeID, MessageNode),
    UpdateStorage(DHTStorageBucket),
}

/// These are the messages which will be exchanged between the nodes for this
/// module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageNode {
    LocalFlos(Vec<FloMeta>),
    RequestFlos(Vec<FloMeta>),
    UpdateFlos(Vec<DHTFlo>),
}

/// The message handling part, but only for DHTStorage messages.
#[derive(Debug)]
pub struct DHTStorageMessages {
    pub core: DHTStorageCore,
    nodes: NodeIDs,
    our_id: NodeID,
}

impl DHTStorageMessages {
    /// Returns a new chat module.
    pub fn new(
        storage: DHTStorageBucket,
        cfg: DHTStorageConfig,
        our_id: NodeID,
    ) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            core: DHTStorageCore::new(storage, cfg),
            nodes: NodeIDs::empty(),
            our_id,
        })
    }

    /// Processes one generic message and returns either an error
    /// or a Vec<MessageOut>.
    pub fn process_messages(&mut self, msgs: Vec<DHTStorageIn>) -> Vec<DHTStorageOut> {
        let mut out = vec![];
        for msg in msgs {
            log::trace!("Got msg: {msg:?}");
            out.extend(match msg {
                DHTStorageIn::Node(src, node_msg) => self.process_node_message(src, node_msg),
                DHTStorageIn::UpdateNodeList(ids) => self.node_list(ids),
            });
        }
        out
    }

    /// Processes a node to node message and returns zero or more
    /// MessageOut.
    pub fn process_node_message(&mut self, _src: NodeID, msg: MessageNode) -> Vec<DHTStorageOut> {
        match msg {
            MessageNode::LocalFlos(vec) => todo!(),
            MessageNode::RequestFlos(vec) => todo!(),
            MessageNode::UpdateFlos(vec) => todo!(),
        }
        vec![]
    }

    /// Stores the new node list, excluding the ID of this node.
    fn node_list(&mut self, mut ids: NodeIDs) -> Vec<DHTStorageOut> {
        self.nodes = ids.remove_missing(&vec![self.our_id].into());
        vec![]
    }
}

/// Convenience method to reduce long lines.
impl From<DHTStorageIn> for DHTStorageMessage {
    fn from(msg: DHTStorageIn) -> Self {
        DHTStorageMessage::Input(msg)
    }
}

/// Convenience method to reduce long lines.
impl From<DHTStorageOut> for DHTStorageMessage {
    fn from(msg: DHTStorageOut) -> Self {
        DHTStorageMessage::Output(msg)
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
        let storage = DHTStorageBucket::default();
        let mut msg = DHTStorageMessages::new(storage, DHTStorageConfig::default(), id0)?;
        msg.process_messages(vec![DHTStorageIn::UpdateNodeList(ids).into()]);
        // let ret = msg.process_messages(vec![
        //     DHTStorageIn::Node(id1, MessageNode::Increase(2)).into()
        // ]);
        // assert_eq!(2, ret.len());
        // assert!(matches!(
        //     ret[0],
        //     DHTStorageOut::Node(_, MessageNode::Counter(2))
        // ));
        // assert!(matches!(
        //     ret[1],
        //     DHTStorageOut::UpdateStorage(DHTStorageBucket { counter: 2 })
        // ));
        Ok(())
    }
}
