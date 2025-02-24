use flarch::{
    broker::SubsystemHandler,
    data_storage::DataStorage,
    nodeids::{NodeID, NodeIDs, U256},
    platform_async_trait,
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use super::{broker::MODULE_NAME, core::*};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModuleMessage {
    KnownEventIDs(Vec<U256>),
    Events(Vec<Event>),
    RequestEventIDs,
    RequestEvents(Vec<U256>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GossipIn {
    Tick,
    FromNetwork(NodeID, ModuleMessage),
    AddEvent(Event),
    UpdateNodeList(NodeIDs),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GossipOut {
    ToNetwork(NodeID, ModuleMessage),
}

/// The first module to use the random_connections is a copy of the previous
/// chat.
/// Now it holds events of multiple categories and exchanges them between the
/// nodes.
pub struct Messages {
    events: EventsStorage,
    storage: Box<dyn DataStorage + Send>,
    tx: Option<watch::Sender<EventsStorage>>,
    id: NodeID,
    nodes: NodeIDs,
    outstanding: Vec<U256>,
}

impl std::fmt::Debug for Messages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GossipEvents")
            .field("events", &self.events)
            .field("id", &self.id)
            .field("nodes", &self.nodes)
            .field("outstanding", &self.outstanding)
            .finish()
    }
}

impl Messages {
    /// Returns a new chat module.
    pub fn new(
        id: NodeID,
        storage: Box<dyn DataStorage + Send>,
    ) -> (Self, watch::Receiver<EventsStorage>) {
        let mut events = EventsStorage::new();
        let gossip_msgs_str = storage.get(MODULE_NAME).unwrap();
        if !gossip_msgs_str.is_empty() {
            if let Err(e) = events.set(&gossip_msgs_str) {
                log::warn!("Couldn't load gossip messages: {}", e);
            }
        }
        let (storage_tx, storage_rx) = watch::channel(events.clone());
        (
            Self {
                events,
                storage,
                tx: Some(storage_tx),
                id,
                nodes: NodeIDs::empty(),
                outstanding: vec![],
            },
            storage_rx,
        )
    }

    fn store(&mut self) {
        self.tx.clone().map(|tx| {
            tx.send(self.events.clone())
                .is_err()
                .then(|| self.tx = None)
        });
        self.events
            .get()
            .map(|events_str| self.storage.set(MODULE_NAME, &events_str))
            .err()
            .map(|e| log::warn!("Couldn't store gossip: {e:?}"));
    }

    /// Processes many messages at a time.
    pub fn process_messages(
        &mut self,
        msgs: Vec<GossipIn>,
    ) -> Result<Vec<GossipOut>, serde_yaml::Error> {
        let mut out = vec![];
        for msg in msgs {
            out.extend(self.process_message(msg));
        }
        Ok(out)
    }

    /// Processes one generic message and returns the result.
    pub fn process_message(&mut self, msg: GossipIn) -> Vec<GossipOut> {
        // log::trace!("{} got message {:?}", self.id, msg);
        match msg {
            GossipIn::Tick => self.tick(),
            GossipIn::FromNetwork(src, node_msg) => self.process_node_message(src, node_msg),
            GossipIn::AddEvent(ev) => self.add_event(ev),
            GossipIn::UpdateNodeList(ids) => self.node_list(ids),
        }
    }

    /// Processes a node to node message and returns zero or more
    /// MessageOut.
    pub fn process_node_message(&mut self, src: NodeID, msg: ModuleMessage) -> Vec<GossipOut> {
        match msg {
            ModuleMessage::KnownEventIDs(ids) => self.node_known_event_ids(src, ids),
            ModuleMessage::Events(events) => self.node_events(src, events),
            ModuleMessage::RequestEvents(ids) => self.node_request_events(src, ids),
            ModuleMessage::RequestEventIDs => self.node_request_event_list(src),
        }
    }

    /// Adds an event if it's not known yet or not too old.
    /// This will send out the event to all other nodes.
    pub fn add_event(&mut self, event: Event) -> Vec<GossipOut> {
        if self.events.add_event(event.clone()) {
            self.store();
            return self.send_events(self.id, &[event]);
        }
        vec![]
    }

    /// Takes a vector of events and stores the new events. It returns all
    /// events that are new to the system.
    pub fn add_events(&mut self, events: Vec<Event>) -> Vec<Event> {
        events
            .into_iter()
            .inspect(|e| self.outstanding.retain(|os| os != &e.get_id()))
            .filter(|e| self.events.add_event(e.clone()))
            .collect()
    }

    fn send_events(&self, src: NodeID, events: &[Event]) -> Vec<GossipOut> {
        self.nodes
            .0
            .iter()
            .filter(|&&node_id| node_id != src && node_id != self.id)
            .map(|node_id| {
                GossipOut::ToNetwork(
                    *node_id,
                    ModuleMessage::KnownEventIDs(
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
            .filter(|&id| !self.nodes.0.contains(id) && id != &self.id)
            .map(|&id| GossipOut::ToNetwork(id, ModuleMessage::RequestEventIDs))
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
            return vec![GossipOut::ToNetwork(
                src,
                ModuleMessage::RequestEvents(unknown_ids),
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
        let output: Vec<GossipOut> = self.send_events(src, &events_out);
        if !events_out.is_empty() {
            self.store();
        }
        output
    }

    /// Send the events to the other node. One or more of the requested
    /// events might be missing.
    pub fn node_request_events(&mut self, src: NodeID, ids: Vec<U256>) -> Vec<GossipOut> {
        let events: Vec<Event> = self.events.get_events_by_ids(ids);
        if !events.is_empty() {
            vec![GossipOut::ToNetwork(src, ModuleMessage::Events(events))]
        } else {
            vec![]
        }
    }

    /// Returns the list of known events.
    pub fn node_request_event_list(&mut self, src: NodeID) -> Vec<GossipOut> {
        vec![GossipOut::ToNetwork(
            src,
            ModuleMessage::KnownEventIDs(self.events.event_ids()),
        )]
    }

    /// Returns all ids that are not in our storage
    pub fn filter_known_events(&self, eventids: Vec<U256>) -> Vec<U256> {
        let our_ids = self.events.event_ids();
        eventids
            .into_iter()
            .filter(|id| !our_ids.contains(id) && !self.outstanding.contains(id))
            .collect()
    }

    pub fn storage(&self) -> EventsStorage {
        self.events.clone()
    }

    /// Every tick clear the outstanding vector and request new event IDs.
    pub fn tick(&mut self) -> Vec<GossipOut> {
        self.outstanding.clear();
        self.nodes
            .0
            .iter()
            .map(|id| GossipOut::ToNetwork(*id, ModuleMessage::RequestEventIDs))
            .collect()
    }
}

#[platform_async_trait()]
impl SubsystemHandler<GossipIn, GossipOut> for Messages {
    async fn messages(&mut self, msgs: Vec<GossipIn>) -> Vec<GossipOut> {
        msgs.into_iter()
            .flat_map(|msg| self.process_message(msg))
            .collect()
    }
}
