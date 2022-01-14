use serde::{Deserialize, Serialize};

use crate::module::Message;
use crate::module::Module;
use crate::network::{Address, NetworkMessage, Node2NodeMsg};
use crate::random_connections::RandomConnectionsMessage;
use types::{nodeids::U256, data_storage::DataStorage};
use raw::gossip_chat;
use raw::gossip_chat::text_message::TextMessage;

const STORAGE_KEY: &str = "messages";

#[derive(Debug)]
pub enum GossipChatMessage {
    AddMessage(String),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GossipChatNodeMessage {
    KnownMsgIDs(Vec<U256>),
    Messages(Vec<TextMessage>),
    RequestMsgList,
    RequestMessages(Vec<U256>),
}

#[derive(Debug)]
pub struct GossipChat {
    pub module: gossip_chat::Module,
    nodes: Vec<U256>,
}

impl Module for GossipChat {
    fn new(ds: Box<dyn DataStorage>) -> Self {
        Self {
            module: gossip_chat::Module::new(&ds.get(STORAGE_KEY).unwrap()),
            nodes: vec![],
        }
    }

    fn process_message(&mut self, msg: &Message) -> Vec<Message> {
        if let Message::Network(NetworkMessage::Node2Node(n)) = msg {
            if let Address::From(from) = n.id {
                if let Node2NodeMsg::GossipChat(gc) = &n.msg {
                    return self.process_gossip_node_msg(from, gc);
                }
            }
        } else {
            return self.process_gossip_msg(msg);
        }
        vec![]
    }

    fn tick(&mut self) -> Vec<Message> {
        vec![]
    }
}

impl GossipChat {
    fn process_gossip_msg(&mut self, msg: &Message) -> Vec<Message> {
        if let Message::RandomConnections(RandomConnectionsMessage::ConnectedNodes(nodes)) = msg {
            self.nodes = nodes.clone();
            nodes
                .iter()
                .map(|id| {
                    NetworkMessage::node2node(
                        *id,
                        Node2NodeMsg::GossipChat(GossipChatNodeMessage::RequestMsgList),
                    )
                })
                .collect()
        } else {
            vec![]
        }
    }

    fn process_gossip_node_msg(&mut self, from: U256, msg: &GossipChatNodeMessage) -> Vec<Message> {
        match msg {
            GossipChatNodeMessage::KnownMsgIDs(msgids) => {
                // Request messages that are unknown to us. Even though this might return
                // too old messages, after some roundtrips, the other nodes should also have
                // discarded the too old messages.
                vec![NetworkMessage::node2node(
                    from,
                    Node2NodeMsg::GossipChat(GossipChatNodeMessage::RequestMessages(
                        self.module.filter_known_messages(msgids.clone()),
                    )),
                )]
            }
            GossipChatNodeMessage::Messages(msgs) => {
                // Store new messages and inform other nodes of new messages. This could be
                // optimized by keeping other nodes' list of known messages...
                let msgs_new = self.module.add_messages(msgs.clone());
                self.nodes
                    .iter()
                    .filter(|&&id| id != from)
                    .map(|&id| {
                        NetworkMessage::node2node(
                            id,
                            Node2NodeMsg::GossipChat(GossipChatNodeMessage::Messages(
                                msgs_new.clone(),
                            )),
                        )
                    })
                    .collect()
            }
            GossipChatNodeMessage::RequestMsgList => {
                // Return the list of known messages
                vec![NetworkMessage::node2node(
                    from,
                    Node2NodeMsg::GossipChat(GossipChatNodeMessage::KnownMsgIDs(
                        self.module.get_message_ids(),
                    )),
                )]
            }
            GossipChatNodeMessage::RequestMessages(msgids) => {
                // Return the messages as requested
                vec![NetworkMessage::node2node(
                    from,
                    Node2NodeMsg::GossipChat(GossipChatNodeMessage::Messages(
                        msgids
                            .iter()
                            .filter_map(|id| self.module.get_message(id))
                            .collect(),
                    )),
                )]
            }
        }
    }
}
