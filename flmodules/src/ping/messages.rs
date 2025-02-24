use flarch::{
    broker::SubsystemHandler,
    nodeids::{NodeID, NodeIDs},
    platform_async_trait,
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use super::core::PingStorage;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModuleMessage {
    Ping,
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PingMessage {
    Input(PingIn),
    Output(PingOut),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PingIn {
    Tick,
    FromNetwork(NodeID, ModuleMessage),
    UpdateNodeList(NodeIDs),
    DisconnectNode(NodeID),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PingOut {
    ToNetwork(NodeID, ModuleMessage),
    Failed(NodeID),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PingConfig {
    // How many ticks between two pings
    pub interval: u32,
    // How many ticks before a missing ping is counted as disconnection
    pub timeout: u32,
}

pub struct Messages {
    storage: PingStorage,
    tx: Option<watch::Sender<PingStorage>>,
}

impl Messages {
    pub fn new(config: PingConfig) -> (Self, watch::Receiver<PingStorage>) {
        let storage = PingStorage::new(config);
        let (storage_tx, rx) = watch::channel(storage.clone());
        (
            Self {
                storage,
                tx: Some(storage_tx),
            },
            rx,
        )
    }

    fn process_msg(&mut self, msg: PingIn) -> Vec<PingOut> {
        match msg {
            PingIn::Tick => self.tick(),
            PingIn::FromNetwork(id, msg_node) => self.message(id, msg_node),
            PingIn::UpdateNodeList(ids) => self.set_nodes(ids),
            PingIn::DisconnectNode(id) => {
                self.storage.remove_node(&id);
                vec![]
            }
        }
    }

    fn tick(&mut self) -> Vec<PingOut> {
        self.storage.tick();
        self.tx.clone().map(|tx| {
            tx.send(self.storage.clone())
                .is_err()
                .then(|| self.tx = None)
        });
        self.create_messages()
    }

    fn message(&mut self, id: NodeID, msg: ModuleMessage) -> Vec<PingOut> {
        match msg {
            ModuleMessage::Ping => {
                if self.storage.stats.contains_key(&id) {
                    vec![PingOut::ToNetwork(id, ModuleMessage::Pong)]
                } else {
                    vec![]
                }
            }
            ModuleMessage::Pong => {
                self.storage.pong(id);
                vec![]
            }
        }
    }

    fn set_nodes(&mut self, ids: NodeIDs) -> Vec<PingOut> {
        self.storage.set_nodes(ids);
        vec![]
    }

    fn create_messages(&mut self) -> Vec<PingOut> {
        let mut out = vec![];
        for id in self.storage.ping.drain(..) {
            out.push(PingOut::ToNetwork(id, ModuleMessage::Ping));
        }
        for id in self.storage.failed.drain(..) {
            out.push(PingOut::Failed(id));
        }

        out
    }
}

#[platform_async_trait()]
impl SubsystemHandler<PingIn, PingOut> for Messages {
    async fn messages(&mut self, msgs: Vec<PingIn>) -> Vec<PingOut> {
        msgs.into_iter()
            .flat_map(|msg| self.process_msg(msg))
            .collect()
    }
}

impl From<PingIn> for PingMessage {
    fn from(msg: PingIn) -> Self {
        PingMessage::Input(msg)
    }
}

impl From<PingOut> for PingMessage {
    fn from(msg: PingOut) -> Self {
        PingMessage::Output(msg)
    }
}

impl Default for PingConfig {
    fn default() -> Self {
        Self {
            interval: 5,
            timeout: 10,
        }
    }
}
