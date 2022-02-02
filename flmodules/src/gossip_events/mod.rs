use serde::{Deserialize, Serialize};
use flutils::nodeids::{NodeID, NodeIDs, U256};

pub mod conversions;
pub mod events;
use events::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageNode {
    KnownMsgIDs(Vec<U256>),
    Events(Vec<Event>),
    RequestMsgIDs,
    RequestEvents(Vec<U256>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageIn {
    Tick,
    Node(NodeID, MessageNode),
    SetStorage(String),
    GetStorage,
    AddEvent(Event),
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
    pub our_id: NodeID,
}

impl Config {
    pub fn new(our_id: NodeID) -> Self {
        Self {
            our_id,
        }
    }
}

/// The first module to use the random_connections is a copy of the previous
/// chat.
/// Now it holds events of multiple categories and exchanges them between the
/// nodes.
#[derive(Debug)]
pub struct Module {
    storage: EventsStorage,
    cfg: Config,
    nodes: NodeIDs,
}

impl Module {
    /// Returns a new chat module.
    pub fn new(cfg: Config) -> Self {
        Self {
            storage: EventsStorage::new(),
            cfg,
            nodes: NodeIDs::empty(),
        }
    }

    /// Processes one generic message and returns either an error
    /// or a Vec<MessageOut>.
    pub fn process_message(
        &mut self,
        msg: MessageIn,
    ) -> Result<Vec<MessageOut>, serde_yaml::Error> {
        log::trace!("{} got message {:?}", self.cfg.our_id, msg);
        Ok(match msg {
            MessageIn::Tick => self.tick(),
            MessageIn::Node(src, node_msg) => self.process_node_message(src, node_msg),
            MessageIn::AddEvent(msg) => self.add_event(msg),
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
            MessageNode::Events(msgs) => self.node_events(src, msgs),
            MessageNode::RequestEvents(ids) => self.node_request_events(src, ids),
            MessageNode::RequestMsgIDs => self.node_request_event_list(src),
        }
    }

    /// Adds an event if it's not known yet or not too old.
    /// This will send out the event to all other nodes.
    pub fn add_event(&mut self, msg: Event) -> Vec<MessageOut> {
        if self.storage.add_event(msg.clone()) {
            return itertools::concat([
                self.send_event(self.cfg.our_id, &msg),
                vec![MessageOut::Updated],
            ]);
        }
        vec![]
    }

    /// Takes a vector of events and stores the new events. It returns all
    /// events that are new to the system.
    pub fn add_events(&mut self, msgs: Vec<Event>) -> Vec<Event> {
        msgs.into_iter()
            .filter(|msg| self.storage.add_event(msg.clone()))
            .collect()
    }

    fn send_event(&self, src: NodeID, msg: &Event) -> Vec<MessageOut> {
        self.nodes
            .0
            .iter()
            .filter(|&&id| id != src && id != msg.src && id != self.cfg.our_id)
            .map(|id| MessageOut::Node(*id, MessageNode::Events(vec![msg.clone()])))
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

    /// Set the event store
    pub fn set(&mut self, data: &str) -> Result<(), serde_yaml::Error> {
        self.storage.set(data)
    }

    /// Get the event store as a string
    pub fn get(&mut self) -> Result<String, serde_yaml::Error> {
        self.storage.get()
    }

    /// Reply with a list of events this node doesn't know yet.
    /// We suppose that if there are too old events in here, they will be
    /// discarded over time.
    pub fn node_known_msg_ids(&mut self, src: NodeID, ids: Vec<U256>) -> Vec<MessageOut> {
        let unknown_ids = self.filter_known_events(ids);
        if !unknown_ids.is_empty() {
            return vec![MessageOut::Node(
                src,
                MessageNode::RequestEvents(unknown_ids),
            )];
        }
        vec![]
    }

    /// Store the new eventss and send them to the other nodes.
    pub fn node_events(&mut self, src: NodeID, msgs: Vec<Event>) -> Vec<MessageOut> {
        // Attention: self.send_event can return an empty vec in case there are no
        // other nodes available yet. So it's not enough to check the 'output' variable
        // to know if the MessageOut::Updated needs to be sent or not.
        let msgs_out = self.add_events(msgs);
        let mut output: Vec<MessageOut> = msgs_out
            .iter()
            .flat_map(|msg| self.send_event(src, msg))
            .collect();
        if !msgs_out.is_empty() {
            output.push(MessageOut::Updated);
        }
        output
    }

    /// Send the events to the other node. One or more of the requested
    /// events might be missing.
    pub fn node_request_events(&mut self, src: NodeID, ids: Vec<U256>) -> Vec<MessageOut> {
        let msgs: Vec<Event> = self.storage.get_events_by_ids(ids);
        if !msgs.is_empty() {
            vec![MessageOut::Node(src, MessageNode::Events(msgs))]
        } else {
            vec![]
        }
    }

    /// Returns the list of known events.
    pub fn node_request_event_list(&mut self, src: NodeID) -> Vec<MessageOut> {
        vec![MessageOut::Node(
            src,
            MessageNode::KnownMsgIDs(self.storage.get_event_ids()),
        )]
    }

    /// Returns all ids that are not in our storage
    pub fn filter_known_events(&self, msgids: Vec<U256>) -> Vec<U256> {
        let our_ids = self.storage.get_event_ids();
        msgids
            .into_iter()
            .filter(|id| !our_ids.contains(id))
            .collect()
    }

    /// Gets a copy of all events stored in the module.
    pub fn get_chat_events(&self, cat: Category) -> Vec<Event> {
        self.storage.get_events(cat)
    }

    /// Gets all event-ids that are stored in the module.
    pub fn get_event_ids(&self) -> Vec<U256> {
        self.storage.get_event_ids()
    }

    /// Gets a single event of the module.
    pub fn get_event(&self, id: &U256) -> Option<Event> {
        self.storage.get_event(id)
    }

    /// Returns all events from a given category
    pub fn get_events(&self, cat: Category) -> Vec<Event> {
        self.storage.get_events(cat)
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
    fn test_new_events() -> Result<(), Error> {
        Ok(())
    }
}
