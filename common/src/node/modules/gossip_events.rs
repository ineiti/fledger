use std::sync::Arc;
use std::sync::Mutex;

use raw::gossip_events::events;
pub use raw::gossip_events::{MessageIn, MessageNode, MessageOut};
use types::{data_storage::DataStorage, utils::now};

use super::random_connections::RandomMessage;
use super::text_messages_v1::TextMessagesStorage;
use crate::{
    broker::{Subsystem, SubsystemListener},
    node::modules::messages::{Message, MessageV1},
    node::timer::BrokerTimer,
    node::NodeData,
    node::{modules::messages::NodeMessage, network::BrokerNetwork},
    node::{BrokerMessage, BrokerModules},
};

#[derive(Debug, Clone, PartialEq)]
pub enum GossipMessage {
    MessageIn(MessageIn),
    MessageOut(MessageOut),
}

impl From<GossipMessage> for BrokerModules {
    fn from(msg: GossipMessage) -> Self {
        Self::Gossip(msg)
    }
}

impl From<MessageIn> for GossipMessage {
    fn from(msg: MessageIn) -> Self {
        Self::MessageIn(msg)
    }
}

impl From<MessageOut> for GossipMessage {
    fn from(msg: MessageOut) -> Self {
        Self::MessageOut(msg)
    }
}

/// This is a wrapper around the raw::gossip_events module. It parses the
/// BrokerMessages for messages of other nodes and for a new NodeList sent by the
/// random_connections module.
pub struct GossipChat {
    node_data: Arc<Mutex<NodeData>>,
    data_storage: Box<dyn DataStorage>,
}

const STORAGE_GOSSIP_EVENTS: &str = "gossip_events";

impl GossipChat {
    pub fn start(node_data: Arc<Mutex<NodeData>>) {
        let (mut broker, mut data_storage) = {
            let mut nd = node_data.lock().unwrap();

            let data_storage = nd.storage.get("fledger");
            let gossip_msgs_str = data_storage.get(STORAGE_GOSSIP_EVENTS).unwrap();
            if !gossip_msgs_str.is_empty() {
                if let Err(e) = nd.gossip_events.set(&gossip_msgs_str) {
                    log::warn!("Couldn't load gossip messages: {}", e);
                }
            } else {
                log::info!("Migrating from old TextMessageStorage to new one.");
                let mut messages = TextMessagesStorage::new();
                if let Err(e) = messages.load(&nd.storage.get("").get("text_message").unwrap()) {
                    log::warn!("Error while loading messages: {}", e);
                } else {
                    let msgs = messages
                        .storage
                        .values()
                        .map(|msg| raw::gossip_events::events::Event {
                            category: raw::gossip_events::events::Category::TextMessage,
                            src: msg.src,
                            created: msg.created,
                            msg: msg.msg.clone(),
                        })
                        .collect();
                    nd.gossip_events.add_events(msgs);
                }
            }
            let msg = events::Event {
                category: events::Category::NodeInfo,
                src: nd.node_config.our_node.get_id(),
                created: now(),
                msg: serde_json::to_string(&nd.node_config.our_node).unwrap(),
            };
            nd.gossip_events.add_event(msg);
            (nd.broker.clone(), data_storage)
        };
        Self::save(&mut data_storage, Arc::clone(&node_data));
        broker
            .add_subsystem(Subsystem::Handler(Box::new(Self {
                node_data,
                data_storage,
            })))
            .unwrap();
    }

    // Searches for a matching NodeMessageIn or a RandomMessage that needs conversion.
    fn process_msg_bm(&mut self, msg: &BrokerMessage) -> Vec<BrokerMessage> {
        match msg {
            BrokerMessage::Network(BrokerNetwork::NodeMessageIn(nm)) => match &nm.msg {
                Message::V1(MessageV1::GossipChat(gc)) => Some(MessageIn::Node(nm.id, gc.clone())),
                _ => None,
            },
            BrokerMessage::Modules(BrokerModules::Random(RandomMessage::MessageOut(msg_rnd))) => {
                msg_rnd.clone().into()
            }
            BrokerMessage::Timer(BrokerTimer::Minute) => Some(MessageIn::Tick),
            _ => None,
        }
        .map(|msg| self.process_msg_in(&msg))
        .unwrap_or_default()
    }

    fn process_msg_in(&mut self, msg: &MessageIn) -> Vec<BrokerMessage> {
        let msgs_res = if let Ok(mut nd) = self.node_data.try_lock() {
            nd.gossip_events.process_message(msg.clone())
        } else {
            log::error!("Couldn't lock");
            return vec![];
        };
        if let Ok(msgs) = msgs_res {
            return msgs
                .iter()
                .map(|msg| match msg {
                    MessageOut::Node(id, nm) => NodeMessage {
                        id: *id,
                        msg: nm.clone().into(),
                    }
                    .output(),
                    MessageOut::Updated => {
                        Self::save(&mut self.data_storage, Arc::clone(&self.node_data));
                        msg.clone().into()
                    }
                    _ => msg.clone().into(),
                })
                .collect();
        }
        vec![]
    }

    fn save(ds: &mut Box<dyn DataStorage>, node_data: Arc<Mutex<NodeData>>) {
        if let Ok(mut nd) = node_data.try_lock() {
            match nd.gossip_events.get() {
                Ok(messages) => {
                    if let Err(e) = ds.set(STORAGE_GOSSIP_EVENTS, &messages) {
                        log::error!("Couldn't store gossip-messages: {}", e);
                    }
                }
                Err(e) => log::error!("Couldn't get messages: {:?}", e),
            }
        }
    }
}

impl SubsystemListener<BrokerMessage> for GossipChat {
    fn messages(&mut self, msgs: Vec<&BrokerMessage>) -> Vec<BrokerMessage> {
        let output = msgs
            .iter()
            .flat_map(|msg| {
                if let BrokerMessage::Modules(BrokerModules::Gossip(GossipMessage::MessageIn(
                    msg_in,
                ))) = msg
                {
                    self.process_msg_in(msg_in)
                } else {
                    self.process_msg_bm(msg)
                }
            })
            .collect();
        output
    }
}
