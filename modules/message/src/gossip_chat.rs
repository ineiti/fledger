use serde::{Deserialize, Serialize};

use crate::module::DataStorage;
use crate::module::Intern;
use crate::module::Address;
use crate::module::Message;
use crate::module::Module;
use crate::module::Node2NodeMsg;
use common::types::U256;
use raw::gossip_chat;
use raw::gossip_chat::text_message::TextMessage;

#[derive(Debug)]
pub enum GossipChatMessage {}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GossipChatNodeMessage {
    KnownMsgIDs(Vec<U256>),
    Messages(Vec<TextMessage>),
    RequestMsgList,
    RequestMessages(Vec<U256>),
}

#[derive(Debug)]
pub struct GossipChat {
    pub gossip_chat: gossip_chat::Module,
}

impl Module for GossipChat {
    fn new(_: Box<dyn DataStorage>) -> Self {
        Self {
            gossip_chat: gossip_chat::Module::new(),
        }
    }

    fn process_message(&mut self, msg: &Message) -> Vec<Message> {
        match msg {
            Message::Intern(i) => return self.process_gossip_msg(i),
            Message::Node2Node(n) => {
                if let Address::From(from) = n.id {
                    if let Node2NodeMsg::GossipChat(gc) = &n.msg {
                        return self.process_gossip_node_msg(from, gc)
                    }
                }
            }
        }
        vec![]
    }

    fn tick(&mut self) -> Vec<Message> {
        todo!()
    }
}

impl GossipChat {
    fn process_gossip_msg(&mut self, msg: &Intern) -> Vec<Message> {
        vec![]
    }

    fn process_gossip_node_msg(&mut self, from: U256, msg: &GossipChatNodeMessage) -> Vec<Message> {
        match msg {
            GossipChatNodeMessage::KnownMsgIDs(msgids) => {}
            GossipChatNodeMessage::Messages(msgs) => {}
            GossipChatNodeMessage::RequestMsgList => {}
            GossipChatNodeMessage::RequestMessages(msgids) => {}
        }
        vec![]
    }
}
