use crate::gossip_chat::GossipChatNodeMessage;
use crate::random_connections::RandomConnectionsNodeMessage;
use serde::{Deserialize, Serialize};
use tracing;

use crate::gossip_chat::GossipChat;
use crate::module::DataStorageBase;
use crate::module::Message;
use crate::module::Module;
use crate::random_connections::RandomConnections;
use common::types::U256;

#[derive(Debug)]
/// This module is used to parse incoming node2node messages.
/// It supposes that the messages are already parsed and filtered, so that only
/// messages to our node are sent here.
/// Additionally, a `tick` method can be called in regular periods to get timeouts
/// running.
pub struct Network {
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
pub enum NetworkMessage {
    Connect(U256),
    Disconnect(Vec<U256>),
    NewConnection(U256),
    AvailableNodes(Vec<U256>),
    Node2Node(Node2Node),
}

impl Network {
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
            if let Some(msg) = msgs.get_intern_msg() {
                msgs.append(self.gossip_chat.process_message(&msg));
                msgs.append(self.random_connections.process_message(&msg));
            } else {
                break;
            }
        }
        msgs.get_network_msgs()
    }
}

/// Messages is used to easily iterate over the internal messages while
/// gathering node2node and network messages.
struct Messages {
    // Messages broadcasted among modules
    intern: Vec<Message>,
    // Network messages
    network: Vec<MessageToNetwork>,
}

impl Messages {
    fn new() -> Self {
        Self {
            intern: vec![],
            network: vec![],
        }
    }

    fn append_from_network(&mut self, msg: MessageFromNetwork) {
        match msg {
            MessageFromNetwork::NewConnection(id) => self
                .intern
                .push(Message::Network(NetworkMessage::NewConnection(id))),
            MessageFromNetwork::Msg(id, msg_str) => {
                if let Ok(msg) = serde_json::from_str::<Node2NodeMsg>(&msg_str) {
                    self.intern
                        .push(Message::Network(NetworkMessage::Node2Node(Node2Node {
                            id: Address::From(id),
                            msg,
                        })));
                } else {
                    tracing::warn!("Unknown message from {:?}: {}", id, msg_str);
                }
            }
            MessageFromNetwork::AvailableNodes(ids) => self
                .intern
                .push(Message::Network(NetworkMessage::AvailableNodes(ids))),
        }
    }

    fn append(&mut self, msgs: Vec<Message>) {
        for msg in msgs {
            if let Message::Network(n) = msg {
                if let Some(msg_net) = n.to_network() {
                    self.network.push(msg_net)
                }
            }
        }
    }

    fn get_intern_msg(&mut self) -> Option<Message> {
        if self.intern.len() > 0 {
            Some(self.intern.remove(0))
        } else {
            None
        }
    }

    fn get_network_msgs(&mut self) -> Vec<MessageToNetwork> {
        self.network.drain(..).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node2Node {
    pub id: Address,
    pub msg: Node2NodeMsg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Address {
    From(U256),
    To(U256),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Node2NodeMsg {
    GossipChat(GossipChatNodeMessage),
    RandomConnections(RandomConnectionsNodeMessage),
}

impl NetworkMessage {
    pub fn node2node(to: U256, msg: Node2NodeMsg) -> Message {
        Message::Network(NetworkMessage::Node2Node(Node2Node {
            id: Address::To(to),
            msg,
        }))
    }

    pub fn to_network(&self) -> Option<MessageToNetwork> {
        match self {
            NetworkMessage::Connect(id) => Some(MessageToNetwork::Connect(*id)),
            NetworkMessage::Disconnect(ids) => Some(MessageToNetwork::Disconnect(ids.clone())),
            NetworkMessage::Node2Node(msg) => {
                if let Address::To(id) = msg.id {
                    let msg_str = serde_json::to_string(&msg.msg).unwrap();
                    Some(MessageToNetwork::Msg(id, msg_str))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}
