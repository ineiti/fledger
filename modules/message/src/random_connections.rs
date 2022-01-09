use common::types::U256;
use serde::{Deserialize, Serialize};

use raw::random_connections;

use crate::module::DataStorage;
use crate::module::Message;
use crate::module::Module;
use crate::network::NetworkMessage;

#[derive(Debug)]
pub enum RandomConnectionsMessage {
    ConnectedNodes(Vec<U256>),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RandomConnectionsNodeMessage {}

#[derive(Debug)]
pub struct RandomConnections {
    module: random_connections::Module,
}

impl Module for RandomConnections {
    fn new(_: Box<dyn DataStorage>) -> Self {
        Self {
            module: random_connections::Module::new(None),
        }
    }

    fn process_message(&mut self, msg: &Message) -> std::vec::Vec<Message> {
        self.process_gossip_msg(msg)
    }

    fn tick(&mut self) -> std::vec::Vec<Message> {
        vec![Message::RandomConnections(
            RandomConnectionsMessage::ConnectedNodes(self.module.connected().0),
        )]
    }
}

impl RandomConnections {
    fn process_gossip_msg(&mut self, msg: &Message) -> Vec<Message> {
        if let Message::Network(conn) = msg {
            return match conn {
                NetworkMessage::AvailableNodes(nodes) => {
                    RandomConnections::dis_connect_msg(self.module.new_nodes(&nodes.into()))
                }
                NetworkMessage::NewConnection(node) => {
                    RandomConnections::dis_connect_msg(self.module.new_connection(node.into()))
                }
                _ => {
                    vec![]
                }
            };
        }
        vec![]
    }

    fn dis_connect_msg(msg: random_connections::Message) -> Vec<Message> {
        msg.0
             .0
            .iter()
            .map(|id| Message::Network(NetworkMessage::Connect(*id)))
            .chain(vec![Message::Network(NetworkMessage::Disconnect(msg.1 .0))])
            .collect()
    }
}
