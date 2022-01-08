use serde::{Deserialize, Serialize};

use raw::random_connections;

use crate::connections::ConnectionsMessage;
use crate::module::DataStorage;
use crate::module::Intern;
use crate::module::Message;
use crate::module::Module;

#[derive(Debug)]
pub enum RandomConnectionsMessage {}
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
        if let Message::Intern(i) = msg {
            self.process_gossip_msg(i)
        } else {
            vec![]
        }
    }

    fn tick(&mut self) -> std::vec::Vec<Message> {
        todo!()
    }
}

impl RandomConnections {
    fn process_gossip_msg(&mut self, msg: &Intern) -> Vec<Message> {
        if let Intern::Connections(conn) = msg {
            return match conn {
                ConnectionsMessage::AvailableNodes(nodes) => {
                    RandomConnections::dis_connect_msg(self.module.new_nodes(&nodes.into()))
                }
                ConnectionsMessage::NewConnection(node) => {
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
            .map(|id| Message::Intern(Intern::Connections(ConnectionsMessage::Connect(*id))))
            .chain(vec![Message::Intern(Intern::Connections(
                ConnectionsMessage::Disconnect(msg.1 .0),
            ))])
            .collect()
    }
}
