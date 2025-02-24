use flarch::{
    broker::SubsystemHandler,
    data_storage::DataStorage,
    nodeids::{NodeID, NodeIDs},
    platform_async_trait,
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::gossip_events::broker::MODULE_NAME;

use super::core::*;

/// These are the messages which will be exchanged between the nodes for this
/// module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModuleMessage {
    Increase(u32),
    Counter(u32),
}

/// The messages here represent all possible interactions with this module.
#[derive(Debug, Clone)]
pub enum TemplateIn {
    FromNetwork(NodeID, ModuleMessage),
    UpdateNodeList(NodeIDs),
}

#[derive(Debug, Clone)]
pub enum TemplateOut {
    ToNetwork(NodeID, ModuleMessage),
}

/// The message handling part, but only for template messages.
#[derive(Debug)]
pub struct Messages {
    core: TemplateCore,
    storage: Box<dyn DataStorage + Send>,
    nodes: NodeIDs,
    our_id: NodeID,
    ts_tx: Option<watch::Sender<TemplateStorage>>,
}

impl Messages {
    /// Returns a new chat module.
    pub fn new(
        storage: Box<dyn DataStorage + Send>,
        cfg: TemplateConfig,
        our_id: NodeID,
    ) -> (Self, watch::Receiver<TemplateStorage>) {
        let str = storage.get(MODULE_NAME).unwrap_or("".into());
        let ts = TemplateStorageSave::from_str(&str).unwrap_or_default();
        let (ts_tx, ts_rx) = watch::channel(ts.clone());
        (
            Self {
                storage,
                core: TemplateCore::new(ts, cfg),
                nodes: NodeIDs::empty(),
                our_id,
                ts_tx: Some(ts_tx),
            },
            ts_rx,
        )
    }

    /// Processes one generic message and returns either an error
    /// or a Vec<MessageOut>.
    pub fn process_messages(&mut self, msgs: Vec<TemplateIn>) -> Vec<TemplateOut> {
        let mut out = vec![];
        for msg in msgs {
            log::trace!("Got msg: {msg:?}");
            out.extend(match msg {
                TemplateIn::FromNetwork(src, node_msg) => self.process_node_message(src, node_msg),
                TemplateIn::UpdateNodeList(ids) => self.node_list(ids),
            });
        }
        out
    }

    /// Processes a node to node message and returns zero or more
    /// MessageOut.
    pub fn process_node_message(&mut self, _src: NodeID, msg: ModuleMessage) -> Vec<TemplateOut> {
        match msg {
            ModuleMessage::Increase(c) => {
                // When increasing the counter, send 'self' counter to all other nodes.
                // Also send a StorageUpdate message.
                self.core.increase(c);
                self.store();
                return self
                    .nodes
                    .0
                    .iter()
                    .map(|id| {
                        TemplateOut::ToNetwork(
                            id.clone(),
                            ModuleMessage::Counter(self.core.storage.counter),
                        )
                    })
                    .collect();
            }
            ModuleMessage::Counter(c) => log::info!("Got counter from {}: {}", _src, c),
        }
        vec![]
    }

    /// Stores the new node list, excluding the ID of this node.
    fn node_list(&mut self, mut ids: NodeIDs) -> Vec<TemplateOut> {
        self.nodes = ids.remove_missing(&vec![self.our_id].into());
        vec![]
    }

    fn store(&mut self) {
        self.ts_tx.clone().map(|tx| {
            tx.send(self.core.storage.clone())
                .is_err()
                .then(|| self.ts_tx = None)
        });
        if let Ok(val) = self.core.storage.to_yaml() {
            self.storage
                .set(MODULE_NAME, &val)
                .err()
                .map(|e| log::warn!("Error while updating storage: {e:?}"));
        }
    }
}

#[platform_async_trait()]
impl SubsystemHandler<TemplateIn, TemplateOut> for Messages {
    async fn messages(&mut self, msgs: Vec<TemplateIn>) -> Vec<TemplateOut> {
        self.process_messages(msgs)
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use flarch::data_storage::DataStorageTemp;

    use super::*;

    #[test]
    fn test_messages() -> Result<(), Box<dyn Error>> {
        let ids = NodeIDs::new(2);
        let id0 = *ids.0.get(0).unwrap();
        let id1 = *ids.0.get(1).unwrap();
        let (mut msg, rx) =
            Messages::new(DataStorageTemp::new_box(), TemplateConfig::default(), id0);
        msg.process_messages(vec![TemplateIn::UpdateNodeList(ids)]);
        let ret = msg.process_messages(vec![TemplateIn::FromNetwork(
            id1,
            ModuleMessage::Increase(2),
        )]);
        assert_eq!(1, ret.len());
        assert!(matches!(
            ret[0],
            TemplateOut::ToNetwork(_, ModuleMessage::Counter(2))
        ));
        assert_eq!(2, rx.borrow().counter);
        Ok(())
    }
}
