use flarch::nodeids::{NodeID, NodeIDs, U256};
use serde::{Deserialize, Serialize};

use super::core::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageNode {
    KnownEventIDs(Vec<U256>),
    Events(Vec<Event>),
    RequestEventIDs,
    RequestEvents(Vec<U256>),
}

#[derive(Clone, Debug)]
pub enum GossipMessage {
    Input(GossipIn),
    Output(GossipOut),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GossipIn {
    Tick,
    Node(NodeID, MessageNode),
    SetStorage(EventsStorage),
    GetStorage,
    AddEvent(Event),
    NodeList(NodeIDs),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GossipOut {
    Node(NodeID, MessageNode),
    Storage(EventsStorage),
    Updated,
}

#[derive(Debug)]
pub struct Config {
    pub our_id: NodeID,
}

impl Config {
    pub fn new(our_id: NodeID) -> Self {
        Self { our_id }
    }
}

/// The first module to use the random_connections is a copy of the previous
/// chat.
/// Now it holds events of multiple categories and exchanges them between the
/// nodes.
#[derive(Debug)]
pub struct GossipEvents {
    storage: EventsStorage,
    cfg: Config,
    nodes: NodeIDs,
    outstanding: Vec<U256>,
}

impl GossipEvents {
    /// Returns a new chat module.
    pub fn new(cfg: Config) -> Self {
        Self {
            storage: EventsStorage::new(),
            cfg,
            nodes: NodeIDs::empty(),
            outstanding: vec![],
        }
    }

    /// Processes many messages at a time.
    pub fn process_messages(
        &mut self,
        msgs: Vec<GossipIn>,
    ) -> Result<Vec<GossipOut>, serde_yaml::Error> {
        let mut out = vec![];
        for msg in msgs {
            out.extend(self.process_message(msg)?);
        }
        Ok(out)
    }

    /// Processes one generic message and returns either an error
    /// or a Vec<MessageOut>.
    pub fn process_message(&mut self, msg: GossipIn) -> Result<Vec<GossipOut>, serde_yaml::Error> {
        log::trace!("{} got message {:?}", self.cfg.our_id, msg);
        Ok(match msg {
            GossipIn::Tick => self.tick(),
            GossipIn::Node(src, node_msg) => self.process_node_message(src, node_msg),
            GossipIn::AddEvent(ev) => self.add_event(ev),
            GossipIn::NodeList(ids) => self.node_list(ids),
            GossipIn::GetStorage => vec![GossipOut::Storage(self.storage.clone())],
            GossipIn::SetStorage(data) => {
                self.storage = data;
                vec![]
            }
        })
    }

    /// Processes a node to node message and returns zero or more
    /// MessageOut.
    pub fn process_node_message(&mut self, src: NodeID, msg: MessageNode) -> Vec<GossipOut> {
        match msg {
            MessageNode::KnownEventIDs(ids) => self.node_known_event_ids(src, ids),
            MessageNode::Events(events) => self.node_events(src, events),
            MessageNode::RequestEvents(ids) => self.node_request_events(src, ids),
            MessageNode::RequestEventIDs => self.node_request_event_list(src),
        }
    }

    /// Adds an event if it's not known yet or not too old.
    /// This will send out the event to all other nodes.
    pub fn add_event(&mut self, event: Event) -> Vec<GossipOut> {
        if self.storage.add_event(event.clone()) {
            return itertools::concat([
                self.send_events(self.cfg.our_id, &[event]),
                vec![GossipOut::Updated, GossipOut::Storage(self.storage.clone())],
            ]);
        }
        vec![]
    }

    /// Takes a vector of events and stores the new events. It returns all
    /// events that are new to the system.
    pub fn add_events(&mut self, events: Vec<Event>) -> Vec<Event> {
        events
            .into_iter()
            .inspect(|e| self.outstanding.retain(|os| os != &e.get_id()))
            .filter(|e| self.storage.add_event(e.clone()))
            .collect()
    }

    fn send_events(&self, src: NodeID, events: &[Event]) -> Vec<GossipOut> {
        self.nodes
            .0
            .iter()
            .filter(|&&node_id| node_id != src && node_id != self.cfg.our_id)
            .map(|node_id| {
                GossipOut::Node(
                    *node_id,
                    MessageNode::KnownEventIDs(
                        events
                            .iter()
                            .filter_map(|e| {
                                if node_id != &e.src {
                                    Some(e.get_id())
                                } else {
                                    None
                                }
                            })
                            .collect(),
                    ),
                )
            })
            .collect()
    }

    /// If an updated list of nodes is available, send a `RequestEventIDs` to
    /// all new nodes.
    pub fn node_list(&mut self, ids: NodeIDs) -> Vec<GossipOut> {
        let reply = ids
            .0
            .iter()
            .filter(|&id| !self.nodes.0.contains(id) && id != &self.cfg.our_id)
            .map(|&id| GossipOut::Node(id, MessageNode::RequestEventIDs))
            .collect();
        self.nodes = ids;
        reply
    }

    /// Reply with a list of events this node doesn't know yet.
    /// We suppose that if there are too old events in here, they will be
    /// discarded over time.
    pub fn node_known_event_ids(&mut self, src: NodeID, ids: Vec<U256>) -> Vec<GossipOut> {
        let unknown_ids = self.filter_known_events(ids);
        if !unknown_ids.is_empty() {
            self.outstanding.extend(unknown_ids.clone());
            return vec![GossipOut::Node(
                src,
                MessageNode::RequestEvents(unknown_ids),
            )];
        }
        vec![]
    }

    /// Store the new events and send them to the other nodes.
    pub fn node_events(&mut self, src: NodeID, events: Vec<Event>) -> Vec<GossipOut> {
        // Attention: self.send_event can return an empty vec in case there are no
        // other nodes available yet. So it's not enough to check the 'output' variable
        // to know if the MessageOut::Updated needs to be sent or not.
        let events_out = self.add_events(events);
        let mut output: Vec<GossipOut> = self.send_events(src, &events_out);
        if !events_out.is_empty() {
            output.extend(vec![
                GossipOut::Updated,
                GossipOut::Storage(self.storage.clone()),
            ]);
        }
        output
    }

    /// Send the events to the other node. One or more of the requested
    /// events might be missing.
    pub fn node_request_events(&mut self, src: NodeID, ids: Vec<U256>) -> Vec<GossipOut> {
        let events: Vec<Event> = self.storage.get_events_by_ids(ids);
        if !events.is_empty() {
            vec![GossipOut::Node(src, MessageNode::Events(events))]
        } else {
            vec![]
        }
    }

    /// Returns the list of known events.
    pub fn node_request_event_list(&mut self, src: NodeID) -> Vec<GossipOut> {
        vec![GossipOut::Node(
            src,
            MessageNode::KnownEventIDs(self.storage.event_ids()),
        )]
    }

    /// Returns all ids that are not in our storage
    pub fn filter_known_events(&self, eventids: Vec<U256>) -> Vec<U256> {
        let our_ids = self.storage.event_ids();
        eventids
            .into_iter()
            .filter(|id| !our_ids.contains(id) && !self.outstanding.contains(id))
            .collect()
    }

    pub fn storage(&self) -> EventsStorage {
        self.storage.clone()
    }

    /// Every tick clear the outstanding vector and request new event IDs.
    pub fn tick(&mut self) -> Vec<GossipOut> {
        self.outstanding.clear();
        self.nodes
            .0
            .iter()
            .map(|id| GossipOut::Node(*id, MessageNode::RequestEventIDs))
            .collect()
    }
}

impl From<GossipIn> for GossipMessage {
    fn from(msg: GossipIn) -> Self {
        GossipMessage::Input(msg)
    }
}

impl From<GossipOut> for GossipMessage {
    fn from(msg: GossipOut) -> Self {
        GossipMessage::Output(msg)
    }
}
