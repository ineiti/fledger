use crate::broker::{Subsystem, SubsystemListener};
use crate::node::logic::messages::{Message, MessageV1};
use crate::node::logic::text_messages::TextMessagesStorage;
use crate::node::NodeData;
use crate::node::{logic::messages::NodeMessage, network::BrokerNetwork};
use crate::node::{BrokerMessage, ModulesMessage};
use std::sync::Arc;
use std::sync::Mutex;

pub use raw::gossip_chat::{MessageIn, MessageNode, MessageOut};

use super::random_connections::RandomMessage;

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
    pub fn start(node_data: Arc<Mutex<NodeData>>) {
        {
            let mut nd = node_data.lock().unwrap();

            let gossip_msgs_str = nd
                .storage
                .get(STORAGE_GOSSIP_CHAT)
                .get(STORAGE_GOSSIP_CHAT)
                .unwrap();
            if !gossip_msgs_str.is_empty() {
                if let Err(e) = nd.gossip_chat.set(&gossip_msgs_str) {
                    log::warn!("Couldn't load gossip messages: {}", e);
                }
            } else {
                // TODO: re-enable
                // log::info!("Migrating from old TextMessageStorage to new one.");
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
        .add_subsystem(Subsystem::Handler(Box::new(Self { node_data })))
        .unwrap();
    }

    // Searches for a matching NodeMessageIn or a RandomMessage that needs conversion.
    fn process_msg_bm(&self, msg: &BrokerMessage) -> Vec<BrokerMessage> {
        match msg {
            BrokerMessage::Network(BrokerNetwork::NodeMessageIn(nm)) => match &nm.msg {
                Message::V1(MessageV1::GossipChat(gc)) => Some(MessageIn::Node(nm.id, gc.clone())),
                _ => None,
            },
            BrokerMessage::Modules(ModulesMessage::Random(RandomMessage::MessageOut(msg_rnd))) => {
                msg_rnd.clone().into()
            }
            _ => None,
        }
        .map(|msg| self.process_msg_in(&msg))
        .unwrap_or_default()
    }

    fn process_msg_in(&self, msg: &MessageIn) -> Vec<BrokerMessage> {
        if let Ok(mut nd) = self.node_data.try_lock() {
            if let Ok(msgs) = nd.gossip_chat.process_message(msg.clone()) {
                return msgs
                    .iter()
                    .map(|msg| match msg {
                        MessageOut::Node(id, nm) => {
                            BrokerMessage::Network(BrokerNetwork::NodeMessageOut(NodeMessage {
                                id: *id,
                                msg: Message::V1(MessageV1::GossipChat(nm.clone())),
                            }))
                        }
                        _ => BrokerMessage::Modules(ModulesMessage::Gossip(
                            GossipMessage::MessageOut(msg.clone()),
                        )),
                    })
                    .collect();
            }
        }
        vec![]
    }
}

impl SubsystemListener for GossipChat {
    fn messages(&mut self, msgs: Vec<&BrokerMessage>) -> Vec<BrokerMessage> {
        msgs.iter()
            .flat_map(|msg| {
                if let BrokerMessage::Modules(ModulesMessage::Gossip(GossipMessage::MessageIn(
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
