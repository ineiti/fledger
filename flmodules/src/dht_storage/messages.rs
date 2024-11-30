use std::error::Error;

use flarch::{
    broker_io::SubsystemHandler,
    nodeids::{NodeID, NodeIDs}, platform_async_trait,
};
use serde::{Deserialize, Serialize};

use crate::{
    dht_routing::broker::{DHTRoutingIn, DHTRoutingOut},
    flo::dht::{DHTFlo, DHTStorageConfig},
};

use super::{
    broker::{DHTStorageIn, DHTStorageOut},
    core::*,
};

/// The messages here represent all possible interactions with this module.
#[derive(Debug, Clone)]
pub enum InternIn {
    Routing(DHTRoutingOut),
    Storage(DHTStorageIn),
}

#[derive(Debug, Clone)]
pub enum InternOut {
    Routing(DHTRoutingIn),
    Storage(DHTStorageOut),
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
    pub fn process_messages(&mut self, msgs: Vec<InternIn>) -> Vec<InternOut> {
        let mut out = vec![];
        for msg in msgs {
            log::trace!("Got msg: {msg:?}");
            // out.extend(match msg {
            //     // InternIn::Node(src, node_msg) => self.process_node_message(src, node_msg),
            //     // InternIn::UpdateNodeList(ids) => self.node_list(ids),
            // });
        }
        out
    }

    /// Processes a node to node message and returns zero or more
    /// MessageOut.
    pub fn process_node_message(&mut self, _src: NodeID, msg: MessageNode) -> Vec<InternOut> {
        match msg {
            MessageNode::LocalFlos(vec) => todo!(),
            MessageNode::RequestFlos(vec) => todo!(),
            MessageNode::UpdateFlos(vec) => todo!(),
        }
        vec![]
    }

    /// Stores the new node list, excluding the ID of this node.
    fn node_list(&mut self, mut ids: NodeIDs) -> Vec<InternOut> {
        self.nodes = ids.remove_missing(&vec![self.our_id].into());
        vec![]
    }
}

#[platform_async_trait()]
impl SubsystemHandler<InternIn, InternOut> for DHTStorageMessages {
    async fn messages(&mut self, inputs: Vec<InternIn>) -> Vec<InternOut> {
        vec![]
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
