use tracing;

use crate::gossip_chat::GossipChat;
use crate::module::Address;
use crate::module::DataStorageBase;
use crate::module::Intern;
use crate::module::Message;
use crate::module::Module;
use crate::module::Node2Node;
use crate::module::Node2NodeMsg;
use crate::random_connections::RandomConnections;
use common::types::U256;

#[derive(Debug)]
/// This module is used to parse incoming node2node messages.
/// It supposes that the messages are already parsed and filtered, so that only
/// messages to our node are sent here.
/// Additionally, a `tick` method can be called in regular periods to get timeouts
/// running.
pub struct Connections {
    gossip_chat: GossipChat,
    random_connections: RandomConnections,
}

/// Messages coming in from the network
pub enum MessageFromNetwork {
    AvailableNodes(Vec<U256>),
    Msg(U256, String),
    NewConnection(U256),
}

/// Messages going out to the network
pub enum MessageToNetwork {
    Connect(U256),
    Disconnect(Vec<U256>),
    Msg(U256, String),
}

/// Networking messages inside the modules
pub enum ConnectionsMessage {
    Connect(U256),
    Disconnect(Vec<U256>),
    NewConnection(U256),
    AvailableNodes(Vec<U256>),
}

impl Connections {
    pub fn new(dsb: Box<dyn DataStorageBase>) -> Self {
        Self {
            gossip_chat: GossipChat::new(dsb.get("gossip_chat")),
            random_connections: RandomConnections::new(dsb.get("random_connections")),
        }
    }

    pub fn process_message(&mut self, msg_net: MessageFromNetwork) -> Vec<MessageToNetwork> {
        let mut msgs = Messages::new();
        msgs.append_from_network(msg_net);
        self.process_messages(msgs)
    }

    pub fn tick(&mut self) -> std::vec::Vec<MessageToNetwork> {
        let mut msgs = Messages::new();
        msgs.append(self.gossip_chat.tick());
        msgs.append(self.random_connections.tick());
        self.process_messages(msgs)
    }

    fn process_messages(&mut self, mut msgs: Messages) -> Vec<MessageToNetwork> {
        loop {
            if let Some(msg) = msgs.get_intern() {
                msgs.append(self.gossip_chat.process_message(&msg));
                msgs.append(self.random_connections.process_message(&msg));
            } else {
                break;
            }
        }
        msgs.messages_to_network()
    }
}

/// Messages is used to easily iterate over the internal messages while
/// gathering node2node and network messages.
struct Messages {
    // Messages broadcaster among modules
    intern: Vec<Intern>,
    // Control-messages for the network layer
    control: Vec<ConnectionsMessage>,
    // Node to node messages
    node: Vec<Node2Node>,
}

impl Messages {
    fn new() -> Self {
        Self {
            intern: vec![],
            control: vec![],
            node: vec![],
        }
    }

    fn append_from_network(&mut self, msg: MessageFromNetwork) {
        match msg {
            MessageFromNetwork::NewConnection(id) => self
                .intern
                .push(Intern::Connections(ConnectionsMessage::NewConnection(id))),
            MessageFromNetwork::Msg(id, msg_str) => {
                if let Ok(msg) = serde_json::from_str::<Node2NodeMsg>(&msg_str) {
                    self.node.push(Node2Node {
                        id: Address::From(id),
                        msg,
                    });
                } else {
                    tracing::warn!("Unknown message from {:?}: {}", id, msg_str);
                }
            }
            MessageFromNetwork::AvailableNodes(ids) => self
                .intern
                .push(Intern::Connections(ConnectionsMessage::AvailableNodes(ids))),
        }
    }

    fn append(&mut self, msgs: Vec<Message>) {
        for msg in msgs {
            match msg {
                Message::Intern(i) => {
                    if let Intern::Connections(cm) = i {
                        self.control.push(cm);
                    } else {
                        self.intern.push(i)
                    }
                }
                Message::Node2Node(n) => self.node.push(n),
            }
        }
    }

    fn get_intern(&mut self) -> Option<Message> {
        if self.intern.len() > 0 {
            Some(Message::Intern(self.intern.remove(0)))
        } else {
            None
        }
    }

    fn messages_to_network(&self) -> Vec<MessageToNetwork> {
        let mut to_network = vec![];
        for msg in &self.node {
            if let Address::To(to) = msg.id {
                if let Ok(msg_str) = serde_json::to_string(&msg.msg) {
                    to_network.push(MessageToNetwork::Msg(to, msg_str));
                }
            }
        }
        to_network
    }
}
