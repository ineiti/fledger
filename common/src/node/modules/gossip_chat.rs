use crate::broker::{BInput, Subsystem, SubsystemListener};
use crate::node::logic::messages::{Message, MessageV1};
use crate::node::logic::text_messages::TextMessagesStorage;
use crate::node::NodeData;
use crate::node::{logic::messages::NodeMessage, network::BrokerNetwork};
use crate::node::{BrokerMessage, ModulesMessage};
use std::sync::Arc;
use std::sync::Mutex;

use types::nodeids::U256;

pub use raw::gossip_chat::{MessageIn, MessageNode, MessageOut};

#[derive(Debug, Clone, PartialEq)]
pub enum GossipMessage {
    MessageIn(MessageIn),
    MessageOut(MessageOut),
}

/// This is a wrapper around the raw::gossip_chat module. It parses the
/// BrokerMessages for messages of other nodes and for a new NodeList sent by the
/// random_connections module.
pub struct GossipChat {
    node_data: Arc<Mutex<NodeData>>,
}

const STORAGE_GOSSIP_CHAT: &str = "gossip_chat";

impl GossipChat {
    pub fn new(node_data: Arc<Mutex<NodeData>>) {
        {
            let mut nd = node_data.lock().unwrap();

            let gossip_msgs_str = nd
                .storage
                .get(STORAGE_GOSSIP_CHAT)
                .get(STORAGE_GOSSIP_CHAT)
                .unwrap();
            if gossip_msgs_str != "" {
                if let Err(e) = nd.gossip_chat.set(&gossip_msgs_str) {
                    log::warn!("Couldn't load gossip messages: {}", e);
                }
            } else {
                log::info!("Migrating from old TextMessageStorage to new one.");
                let mut messages = TextMessagesStorage::new();
                if let Err(e) =
                    messages.load(&nd.storage.get("something").get("something").unwrap())
                {
                    log::warn!("Error while loading messages: {}", e);
                } else {
                    let msgs = messages
                        .storage
                        .values()
                        .map(|msg| raw::gossip_chat::text_message::TextMessage {
                            src: msg.src,
                            created: msg.created,
                            msg: msg.msg.clone(),
                        })
                        .collect();
                    nd.gossip_chat.add_messages(msgs);
                }
            }
            nd.broker.clone()
        }
        .add_subsystem(Subsystem::Handler(Box::new(Self {
            node_data: node_data,
        })))
        .unwrap();
    }

    // Converts a BrokerMessage to an Option<MessageIn>
    fn process_msg_bm(&self, msg: &BrokerMessage) -> Vec<BrokerMessage> {
        if let Ok(mut nd) = self.node_data.try_lock() {
            match msg {
                BrokerMessage::Network(bmn) => match bmn {
                    // need "Connect" and "Disconnect" here
                    BrokerNetwork::UpdateList(nodes) => Some(MessageIn::NodeList(
                        nodes
                            .iter()
                            .map(|ni| ni.get_id())
                            .collect::<Vec<U256>>()
                            .into(),
                    )),
                    _ => None,
                },
                BrokerMessage::NodeMessage(nm) => match &nm.msg {
                    Message::V1(MessageV1::GossipChat(gc)) => {
                        Some(MessageIn::Node(nm.id, gc.clone()))
                    }
                    _ => None,
                },
                _ => None,
            }
            .and_then(|msg| nd.gossip_chat.process_message(msg).ok())
            .and_then(|msgs| {
                Some(
                    msgs.iter()
                        .flat_map(|msg| self.process_msg_out(msg))
                        .collect(),
                )
            })
            .unwrap_or(vec![])
        } else {
            vec![]
        }
    }

    fn process_msg_in(&self, msg: &MessageIn) -> Vec<BrokerMessage> {
        if let Ok(mut nd) = self.node_data.try_lock() {
            if let Ok(msgs) = nd.gossip_chat.process_message(msg.clone()) {
                return msgs
                    .iter()
                    .map(|msg| {
                        BrokerMessage::Modules(ModulesMessage::Gossip(GossipMessage::MessageOut(
                            msg.clone(),
                        )))
                    })
                    .collect();
            }
        }
        vec![]
    }

    fn process_msg_out(&self, msg: &MessageOut) -> Vec<BrokerMessage> {
        match msg {
            MessageOut::Node(id, nm) => vec![BrokerMessage::NodeMessage(NodeMessage {
                id: id.clone(),
                msg: Message::V1(MessageV1::GossipChat(nm.clone())),
            })],
            _ => vec![],
        }
    }

    fn process_msg(&self, msg: &BrokerMessage) -> Vec<BrokerMessage> {
        match msg {
            BrokerMessage::Modules(ModulesMessage::Gossip(msg)) => match msg {
                GossipMessage::MessageIn(msg) => self.process_msg_in(msg),
                GossipMessage::MessageOut(_) => {
                    log::warn!("This should never receive a MessageOut");
                    vec![]
                },
            },
            _ => self.process_msg_bm(msg),
        }
    }
}

impl SubsystemListener for GossipChat {
    fn messages(&mut self, msgs: Vec<&BrokerMessage>) -> Vec<BInput> {
        msgs.iter()
            .flat_map(|msg| self.process_msg(msg))
            .map(|msg| BInput::BM(msg))
            .collect()
    }
}
