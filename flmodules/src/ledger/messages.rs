use std::error::Error;

use flarch::nodeids::{NodeID, NodeIDs};
use serde::{Deserialize, Serialize};

use super::core::*;

/// These are the messages which will be exchanged between the nodes for this
/// module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageNode {}

/// First wrap all messages coming into this module and all messages going out in
/// a single message time.
#[derive(Clone, Debug)]
pub enum LedgerMessage {
    Input(LedgerIn),
    Output(LedgerOut),
}

/// The messages here represent all possible interactions with this module.
#[derive(Debug, Clone)]
pub enum LedgerIn {
    Node(NodeID, MessageNode),
    UpdateNodeList(NodeIDs),
}

#[derive(Debug, Clone)]
pub enum LedgerOut {
    Node(NodeID, MessageNode),
    UpdateStorage(LedgerStorage),
}

/// The message handling part, but only for template messages.
#[derive(Debug)]
pub struct LedgerMessages {
    pub core: Ledger,
    nodes: NodeIDs,
    our_id: NodeID,
}

impl LedgerMessages {
    /// Returns a new chat module.
    pub fn new(
        storage: LedgerStorage,
        cfg: LedgerConfig,
        our_id: NodeID,
    ) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            core: Ledger::new(storage, cfg),
            nodes: NodeIDs::empty(),
            our_id,
        })
    }

    /// Processes one generic message and returns either an error
    /// or a Vec<MessageOut>.
    pub fn process_messages(&mut self, msgs: Vec<LedgerIn>) -> Vec<LedgerOut> {
        let mut out = vec![];
        for msg in msgs {
            log::trace!("Got msg: {msg:?}");
            out.extend(match msg {
                LedgerIn::Node(src, node_msg) => self.process_node_message(src, node_msg),
                LedgerIn::UpdateNodeList(ids) => self.node_list(ids),
            });
        }
        out
    }

    /// Processes a node to node message and returns zero or more
    /// MessageOut.
    pub fn process_node_message(&mut self, _src: NodeID, _msg: MessageNode) -> Vec<LedgerOut> {
        // match msg {}
        vec![]
    }

    /// Stores the new node list, excluding the ID of this node.
    fn node_list(&mut self, mut ids: NodeIDs) -> Vec<LedgerOut> {
        self.nodes = ids.remove_missing(&vec![self.our_id].into());
        vec![]
    }
}

/// Convenience method to reduce long lines.
impl From<LedgerIn> for LedgerMessage {
    fn from(msg: LedgerIn) -> Self {
        LedgerMessage::Input(msg)
    }
}

/// Convenience method to reduce long lines.
impl From<LedgerOut> for LedgerMessage {
    fn from(msg: LedgerOut) -> Self {
        LedgerMessage::Output(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}
