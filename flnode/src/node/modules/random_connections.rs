use flnet::network::BrokerNetworkCall;
use flnet::network::BrokerNetworkReply;
use flutils::broker::Destination;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::Mutex;

pub use flmodules::random_connections::{MessageIn, MessageOut};
use flutils::{
    broker::{Subsystem, SubsystemListener},
    nodeids::U256,
};

use crate::node::NodeData;

use super::messages::BrokerMessage;
use super::messages::BrokerModules;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RandomMessage {
    MessageIn(MessageIn),
    MessageOut(MessageOut),
}

impl From<RandomMessage> for BrokerModules {
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
    our_id: U256,
}

impl RandomConnections {
    pub fn start(node_data: Arc<Mutex<NodeData>>) {
        let (mut broker, our_id) = {
            let nd = node_data.lock().unwrap();
            (nd.broker.clone(), nd.node_config.our_node.get_id())
        };
        broker
            .add_subsystem(Subsystem::Handler(Box::new(Self { node_data, our_id })))
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
                        vec![BrokerNetworkCall::Connect(*id).into()]
                    }
                    MessageOut::DisconnectNode(id) => {
                        vec![BrokerNetworkCall::Disconnect(*id).into()]
                    }
                    MessageOut::ListUpdate(_) => {
                        vec![BrokerModules::Random(RandomMessage::MessageOut(msg.clone())).into()]
                    }
                })
                .collect();
        } else {
            log::error!("Couldn't lock");
        }
        vec![]
    }

    fn process_msg_bm(&self, msg: &BrokerMessage) -> Vec<BrokerMessage> {
        if let BrokerMessage::NetworkReply(bnr) = msg {
            match bnr {
                BrokerNetworkReply::RcvWSUpdateList(nodes) => vec![MessageIn::NodeList(
                    nodes
                        .iter()
                        .map(|ni| ni.get_id())
                        .filter(|id| *id != self.our_id)
                        .collect::<Vec<U256>>()
                        .into(),
                )],
                BrokerNetworkReply::Connected(id) => vec![MessageIn::NodeConnected(*id)],
                BrokerNetworkReply::Disconnected(id) => {
                    vec![MessageIn::NodeDisconnected(*id)]
                }
                _ => vec![],
            }
        } else {
            vec![]
        }
        .iter()
        .flat_map(|msg| self.process_msg_in(msg))
        .collect()
    }
}

impl SubsystemListener<BrokerMessage> for RandomConnections {
    fn messages(&mut self, msgs: Vec<&BrokerMessage>) -> Vec<(Destination, BrokerMessage)> {
        msgs.iter()
            .flat_map(|msg| {
                if let BrokerMessage::Modules(BrokerModules::Random(RandomMessage::MessageIn(
                    msg_in,
                ))) = msg
                {
                    self.process_msg_in(msg_in)
                } else {
                    self.process_msg_bm(msg)
                }
            })
            .map(|m| (Destination::Others, m))
            .collect()
    }
}
