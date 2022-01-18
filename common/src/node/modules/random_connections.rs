use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::Mutex;
use types::nodeids::U256;

use crate::broker::{BInput, Subsystem, SubsystemListener};
use crate::node::logic::messages::{Message, MessageV1};
use crate::node::NodeData;
use crate::node::{logic::messages::NodeMessage, network::BrokerNetwork};
use crate::node::{BrokerMessage, ModulesMessage};

pub use raw::random_connections::{MessageIn, MessageOut};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RandomMessage {
    MessageIn(MessageIn),
    MessageOut(MessageOut),
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
    pub fn new(node_data: Arc<Mutex<NodeData>>) {
        {
            let nd = node_data.lock().unwrap();
            nd.broker.clone()
        }
        .add_subsystem(Subsystem::Handler(Box::new(Self {
            node_data: node_data,
        })))
        .unwrap();
    }

    fn process_msg_bm(&self, msg: &BrokerMessage) -> Vec<BrokerMessage> {
        if let Ok(mut nd) = self.node_data.try_lock() {
            return match msg {
                BrokerMessage::Network(bmn) => match bmn {
                    BrokerNetwork::UpdateList(nodes) => vec![MessageIn::NodeList(
                        nodes
                            .iter()
                            .map(|ni| ni.get_id())
                            .collect::<Vec<U256>>()
                            .into(),
                    )],
                    BrokerNetwork::Connected(id) => vec![MessageIn::NodeConnected(id.clone())],
                    BrokerNetwork::Disconnected(id) => {
                        vec![MessageIn::NodeDisconnected(id.clone())]
                    }
                    _ => vec![],
                },
                _ => vec![],
            }
            .iter()
            .flat_map(|msg| nd.random_connections.process_message(msg.clone()))
            .flat_map(|msg| self.process_msg_out(&msg))
            .collect();
        }
        vec![]
    }

    fn process_msg_out(&self, msg: &MessageOut) -> Vec<BrokerMessage> {
        match msg {
            MessageOut::ConnectNode(id) => {
                vec![BrokerMessage::Network(BrokerNetwork::Connect(id.clone()))]
            }
            MessageOut::DisconnectNode(id) => {
                vec![BrokerMessage::Network(BrokerNetwork::Disconnect(id.clone()))]
            }
            MessageOut::ListUpdate(_) => vec![BrokerMessage::Modules(ModulesMessage::Random(
                RandomMessage::MessageOut(msg.clone()),
            ))],
        }
    }

    fn process_msg_in(&self, msg: &MessageIn) -> Vec<BrokerMessage> {
        if let Ok(mut nd) = self.node_data.try_lock() {
            return nd
                .random_connections
                .process_message(msg.clone())
                .iter()
                .flat_map(|msg| self.process_msg_out(msg))
                .collect();
        }
        vec![]
    }

    fn process_msg(&self, msg: &BrokerMessage) -> Vec<BrokerMessage> {
        match msg {
            BrokerMessage::Modules(ModulesMessage::Random(msg)) => match msg {
                RandomMessage::MessageIn(msg) => self.process_msg_in(msg),
                RandomMessage::MessageOut(_) => {
                    log::warn!("This module should never get a MessageOut");
                    vec![]
                }
            },
            _ => self.process_msg_bm(msg),
        }
    }
}

impl SubsystemListener for RandomConnections {
    fn messages(&mut self, msgs: Vec<&BrokerMessage>) -> Vec<BInput> {
        msgs.iter()
            .flat_map(|msg| self.process_msg(msg))
            .map(|msg| BInput::BM(msg))
            .collect()
    }
}
