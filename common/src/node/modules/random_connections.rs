use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::Mutex;
use types::nodeids::U256;

use crate::broker::{Subsystem, SubsystemListener};
use crate::node::network::BrokerNetwork;
use crate::node::NodeData;
use crate::node::{BrokerMessage, ModulesMessage};

pub use raw::random_connections::{MessageIn, MessageOut};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RandomMessage {
    MessageIn(MessageIn),
    MessageOut(MessageOut),
}

impl From<RandomMessage> for ModulesMessage {
    fn from(msg: RandomMessage) -> Self {
        Self::Random(msg)
    }
}

/// This is a wrapper to handle BrokerMessages.
/// It translates from BrokerMessages to MessageIn, and from
/// MessageOut to BrokerMessages.
/// All RandomConnections messages are sent through the broker system,
/// so that other modules can interact, too.
pub struct RandomConnections {
    node_data: Arc<Mutex<NodeData>>,
}

impl RandomConnections {
    pub fn start(node_data: Arc<Mutex<NodeData>>) {
        {
            let nd = node_data.lock().unwrap();
            nd.broker.clone()
        }
        .add_subsystem(Subsystem::Handler(Box::new(Self { node_data })))
        .unwrap();
    }

    fn process_msg_in(&self, msg: &MessageIn) -> Vec<BrokerMessage> {
        if let Ok(mut nd) = self.node_data.try_lock() {
            return nd
                .random_connections
                .process_message(msg.clone())
                .iter()
                .flat_map(|msg| match msg {
                    MessageOut::ConnectNode(id) => {
                        vec![BrokerMessage::Network(BrokerNetwork::Connect(*id))]
                    }
                    MessageOut::DisconnectNode(id) => {
                        vec![BrokerMessage::Network(BrokerNetwork::Disconnect(*id))]
                    }
                    MessageOut::ListUpdate(_) => {
                        vec![ModulesMessage::Random(RandomMessage::MessageOut(msg.clone())).into()]
                    }
                })
                .collect();
        }
        vec![]
    }

    fn process_msg_bm(&self, msg: &BrokerMessage) -> Vec<BrokerMessage> {
        match msg {
            BrokerMessage::Network(bmn) => match bmn {
                BrokerNetwork::UpdateList(nodes) => vec![MessageIn::NodeList(
                    nodes
                        .iter()
                        .map(|ni| ni.get_id())
                        .collect::<Vec<U256>>()
                        .into(),
                )],
                BrokerNetwork::Connected(id) => vec![MessageIn::NodeConnected(*id)],
                BrokerNetwork::Disconnected(id) => {
                    vec![MessageIn::NodeDisconnected(*id)]
                }
                _ => vec![],
            },
            _ => vec![],
        }
        .iter()
        .flat_map(|msg| self.process_msg_in(msg))
        .collect()
    }
}

impl SubsystemListener for RandomConnections {
    fn messages(&mut self, msgs: Vec<&BrokerMessage>) -> Vec<BrokerMessage> {
        msgs.iter()
            .flat_map(|msg| {
                if let BrokerMessage::Modules(ModulesMessage::Random(RandomMessage::MessageIn(
                    msg_in,
                ))) = msg
                {
                    self.process_msg_in(msg_in)
                } else {
                    self.process_msg_bm(msg)
                }
            })
            .collect()
    }
}