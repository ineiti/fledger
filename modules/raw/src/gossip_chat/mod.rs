use serde::{Deserialize, Serialize};
use types::nodeids::{NodeID, NodeIDs, U256};

pub mod conversions;
pub mod message;
use message::*;

const MESSAGE_MAXIMUM: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageNode {
    KnownMsgIDs(Vec<U256>),
    Messages(Vec<Message>),
    RequestMsgIDs,
    RequestMessages(Vec<U256>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageIn {
    Tick,
    Node(NodeID, MessageNode),
    SetStorage(String),
    GetStorage,
    AddMessage(Message),
    NodeList(NodeIDs),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageOut {
    Node(NodeID, MessageNode),
    Storage(String),
    Updated,
}

#[derive(Debug)]
pub struct Config {
    pub maximum_messages: usize,
    pub our_id: NodeID,
}

impl Config {
    pub fn new(our_id: NodeID) -> Self {
        Self {
            maximum_messages: MESSAGE_MAXIMUM,
            our_id,
        }
    }
}

/// The first module to use the random_connections is a copy of the previous
/// chat.
/// It makes sure to update from the previous chat messages, and simply
/// copies all messages to all nodes.
/// It keeps 20 messages in memory, each message not being bigger than 1kB.
#[derive(Debug)]
pub struct Module {
    storage: MessageStorage,
    cfg: Config,
    nodes: NodeIDs,
}

impl Module {
    /// Returns a new chat module.
    pub fn new(cfg: Config) -> Self {
        Self {
            storage: MessageStorage::new(),
            cfg,
            nodes: NodeIDs::empty(),
        }
    }

    /// Processes one generic message and returns either an error
    /// or a Vec<MessageOut>.
    pub fn process_message(
        &mut self,
        msg: MessageIn,
    ) -> Result<Vec<MessageOut>, serde_json::Error> {
        log::trace!("{} got message {:?}", self.cfg.our_id, msg);
        Ok(match msg {
            MessageIn::Tick => self.tick(),
            MessageIn::Node(src, node_msg) => self.process_node_message(src, node_msg),
            MessageIn::AddMessage(msg) => self.add_message(msg),
            MessageIn::NodeList(ids) => self.node_list(ids),
            MessageIn::GetStorage => vec![MessageOut::Storage(self.get()?)],
            MessageIn::SetStorage(data) => {
                self.set(&data)?;
                vec![]
            }
        })
    }

    /// Processes a node to node message and returns zero or more
    /// MessageOut.
    pub fn process_node_message(&mut self, src: NodeID, msg: MessageNode) -> Vec<MessageOut> {
        match msg {
            MessageNode::KnownMsgIDs(ids) => self.node_known_msg_ids(src, ids),
            MessageNode::Messages(msgs) => self.node_messages(src, msgs),
            MessageNode::RequestMessages(ids) => self.node_request_messages(src, ids),
            MessageNode::RequestMsgIDs => self.node_request_msg_list(src),
        }
    }

    /// Adds a message if it's not known yet or not too old.
    /// This will send out the message to all other nodes.
    pub fn add_message(&mut self, msg: Message) -> Vec<MessageOut> {
        if self.storage.add_message(msg.clone()) {
            return itertools::concat([
                self.send_message(self.cfg.our_id, &msg),
                vec![MessageOut::Updated],
            ]);
        }
        vec![]
    }

    /// Takes a vector of Messages and stores the new messages. It returns all
    /// messages that are new to the system.
    pub fn add_messages(&mut self, msgs: Vec<Message>) -> Vec<Message> {
        msgs.into_iter()
            .flat_map(|msg| {
                if self.storage.add_message(msg.clone()) {
                    Some(msg)
                } else {
                    None
                }
            })
            .collect()
    }

    fn send_message(&self, src: NodeID, msg: &Message) -> Vec<MessageOut> {
        self.nodes
            .0
            .iter()
            .filter(|&&id| id != src && id != msg.src && id != self.cfg.our_id)
            .map(|id| MessageOut::Node(*id, MessageNode::Messages(vec![msg.clone()])))
            .collect()
    }

    /// If an updated list of nodes is available, send a `RequestMsgIDs` to
    /// all new nodes.
    pub fn node_list(&mut self, ids: NodeIDs) -> Vec<MessageOut> {
        let reply = ids
            .0
            .iter()
            .filter(|&id| !self.nodes.0.contains(id) && id != &self.cfg.our_id)
            .map(|&id| MessageOut::Node(id, MessageNode::RequestMsgIDs))
            .collect();
        self.nodes = ids;
        reply
    }

    /// Set the message store
    pub fn set(&mut self, data: &str) -> Result<(), serde_json::Error> {
        self.storage.set(data)
    }

    /// Get the message store as a string
    pub fn get(&mut self) -> Result<String, serde_json::Error> {
        self.storage.get()
    }

    /// Reply with a list of messages this node doesn't know yet.
    /// We suppose that if there are too old messages in here, they will be
    /// discarded over time.
    pub fn node_known_msg_ids(&mut self, src: NodeID, ids: Vec<U256>) -> Vec<MessageOut> {
        let unknown_ids = self.filter_known_messages(ids);
        if !unknown_ids.is_empty() {
            return vec![MessageOut::Node(
                src,
                MessageNode::RequestMessages(unknown_ids),
            )];
        }
        vec![]
    }

    /// Store the new messages and send them to the other nodes.
    pub fn node_messages(&mut self, src: NodeID, msgs: Vec<Message>) -> Vec<MessageOut> {
        self.add_messages(msgs)
            .iter()
            .flat_map(|msg| self.send_message(src, msg))
            .collect()
    }

    /// Send the messages to the other node. One or more of the requested
    /// messages might be missing.
    pub fn node_request_messages(&mut self, src: NodeID, ids: Vec<U256>) -> Vec<MessageOut> {
        let msgs: Vec<Message> = self.storage.get_messages_by_ids(ids);
        if !msgs.is_empty() {
            vec![MessageOut::Node(src, MessageNode::Messages(msgs))]
        } else {
            vec![]
        }
    }

    /// Returns the list of known messages.
    pub fn node_request_msg_list(&mut self, src: NodeID) -> Vec<MessageOut> {
        vec![MessageOut::Node(
            src,
            MessageNode::KnownMsgIDs(self.storage.get_message_ids()),
        )]
    }

    /// Returns all ids that are not in our storage
    pub fn filter_known_messages(&self, msgids: Vec<U256>) -> Vec<U256> {
        let our_ids = self.storage.get_message_ids();
        msgids
            .into_iter()
            .filter(|id| !our_ids.contains(id))
            .collect()
    }

    /// Gets a copy of all messages stored in the module.
    pub fn get_chat_messages(&self, cat: Category) -> Vec<Message> {
        self.storage.get_messages(cat)
    }

    /// Gets all message-ids that are stored in the module.
    pub fn get_message_ids(&self) -> Vec<U256> {
        self.storage.get_message_ids()
    }

    /// Gets a single message of the module.
    pub fn get_message(&self, id: &U256) -> Option<Message> {
        self.storage.get_message(id)
    }

    /// Nothing to do for tick for the moment.
    pub fn tick(&self) -> Vec<MessageOut> {
        self.nodes
            .0
            .iter()
            .map(|id| MessageOut::Node(*id, MessageNode::RequestMsgIDs))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    // use super::*;
    use core::fmt::Error;

    #[test]
    fn test_new_message() -> Result<(), Error> {
        Ok(())
    }
}
