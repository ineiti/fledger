use flarch::{
    broker::SubsystemHandler,
    data_storage::DataStorage,
    nodeids::{NodeID, NodeIDs, U256},
    platform_async_trait,
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::{
    gossip_events::broker::{GossipIn, GossipOut},
    random_connections::broker::{RandomIn, RandomOut},
    router::messages::NetworkWrapper,
};

use super::{broker::MODULE_NAME, core::*};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModuleMessage {
    KnownEventIDs(Vec<U256>),
    Events(Vec<Event>),
    RequestEventIDs,
    RequestEvents(Vec<U256>),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum InternIn {
    Tick,
    Network(RandomOut),
    Gossip(GossipIn),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum InternOut {
    Network(RandomIn),
    Gossip(GossipOut),
}

/// The first module to use the random_connections is a copy of the previous
/// chat.
/// Now it holds events of multiple categories and exchanges them between the
/// nodes.
pub(super) struct Intern {
    events: EventsStorage,
    storage: Box<dyn DataStorage + Send>,
    tx: Option<watch::Sender<EventsStorage>>,
    id: NodeID,
    nodes: NodeIDs,
    outstanding: Vec<U256>,
    ticks: usize,
}

impl std::fmt::Debug for Intern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GossipEvents")
            .field("events", &self.events)
            .field("id", &self.id)
            .field("nodes", &self.nodes)
            .field("outstanding", &self.outstanding)
            .finish()
    }
}

impl Intern {
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
                ticks: 0,
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

    fn msg_tick(&mut self) -> Vec<InternOut> {
        self.ticks += 1;
        // Magic number - 3 seems to work fine here...
        if self.ticks >= 3 {
            self.tick()
        } else {
            vec![]
        }
    }

    fn msg_net(&mut self, msg: RandomOut) -> Vec<InternOut> {
        match msg {
            RandomOut::NodeIDsConnected(node_ids) => self.node_list(node_ids),
            RandomOut::NetworkWrapperFromNetwork(src, network_wrapper) => network_wrapper
                .unwrap_yaml(MODULE_NAME)
                .map(|msg| self.process_node_message(src, msg))
                .unwrap_or(vec![]),
            _ => vec![],
        }
    }

    fn msg_gossip(&mut self, msg: GossipIn) -> Vec<InternOut> {
        match msg {
            GossipIn::AddEvent(event) => self.add_event(event),
        }
    }

    /// Processes a node to node message and returns zero or more
    /// MessageOut.
    fn process_node_message(&mut self, src: NodeID, msg: ModuleMessage) -> Vec<InternOut> {
        match msg {
            ModuleMessage::KnownEventIDs(ids) => self.node_known_event_ids(src, ids),
            ModuleMessage::Events(events) => self.node_events(src, events),
            ModuleMessage::RequestEvents(ids) => self.node_request_events(src, ids),
            ModuleMessage::RequestEventIDs => self.node_request_event_list(src),
        }
    }

    /// Adds an event if it's not known yet or not too old.
    /// This will send out the event to all other nodes.
    fn add_event(&mut self, event: Event) -> Vec<InternOut> {
        if self.events.add_event(event.clone()) {
            self.store();
            return self.send_events(self.id, &[event]);
        }
        vec![]
    }

    /// Takes a vector of events and stores the new events. It returns all
    /// events that are new to the system.
    fn add_events(&mut self, events: Vec<Event>) -> Vec<Event> {
        events
            .into_iter()
            .inspect(|e| self.outstanding.retain(|os| os != &e.get_id()))
            .filter(|e| self.events.add_event(e.clone()))
            .collect()
    }

    fn send_events(&self, src: NodeID, events: &[Event]) -> Vec<InternOut> {
        self.nodes
            .0
            .iter()
            .filter(|&&node_id| node_id != src && node_id != self.id)
            .flat_map(|node_id| {
                Self::create_network_msg(
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
            .chain(
                events
                    .iter()
                    .map(|e| InternOut::Gossip(GossipOut::NewEvent(e.clone()))),
            )
            .collect()
    }

    /// If an updated list of nodes is available, send a `RequestEventIDs` to
    /// all new nodes.
    fn node_list(&mut self, ids: NodeIDs) -> Vec<InternOut> {
        let reply = ids
            .0
            .iter()
            .filter(|&id| !self.nodes.0.contains(id) && id != &self.id)
            .flat_map(|&id| Self::create_network_msg(id, ModuleMessage::RequestEventIDs))
            .collect();
        self.nodes = ids;
        reply
    }

    /// Reply with a list of events this node doesn't know yet.
    /// We suppose that if there are too old events in here, they will be
    /// discarded over time.
    fn node_known_event_ids(&mut self, src: NodeID, ids: Vec<U256>) -> Vec<InternOut> {
        let unknown_ids = self.filter_known_events(ids);
        if !unknown_ids.is_empty() {
            self.outstanding.extend(unknown_ids.clone());
            return Self::create_network_msg(src, ModuleMessage::RequestEvents(unknown_ids));
        }
        vec![]
    }

    /// Store the new events and send them to the other nodes.
    fn node_events(&mut self, src: NodeID, events: Vec<Event>) -> Vec<InternOut> {
        // Attention: self.send_event can return an empty vec in case there are no
        // other nodes available yet. So it's not enough to check the 'output' variable
        // to know if the MessageOut::Updated needs to be sent or not.
        let events_out = self.add_events(events);
        let output: Vec<InternOut> = self.send_events(src, &events_out);
        if !events_out.is_empty() {
            self.store();
        }
        output
    }

    /// Send the events to the other node. One or more of the requested
    /// events might be missing.
    fn node_request_events(&mut self, src: NodeID, ids: Vec<U256>) -> Vec<InternOut> {
        let events: Vec<Event> = self.events.get_events_by_ids(ids);
        if !events.is_empty() {
            Self::create_network_msg(src, ModuleMessage::Events(events))
        } else {
            vec![]
        }
    }

    /// Returns the list of known events.
    fn node_request_event_list(&mut self, src: NodeID) -> Vec<InternOut> {
        Self::create_network_msg(src, ModuleMessage::KnownEventIDs(self.events.event_ids()))
    }

    /// Returns all ids that are not in our storage
    fn filter_known_events(&self, eventids: Vec<U256>) -> Vec<U256> {
        let our_ids = self.events.event_ids();
        eventids
            .into_iter()
            .filter(|id| !our_ids.contains(id) && !self.outstanding.contains(id))
            .collect()
    }

    /// Every tick clear the outstanding vector and request new event IDs.
    fn tick(&mut self) -> Vec<InternOut> {
        self.outstanding.clear();
        self.nodes
            .0
            .iter()
            .flat_map(|id| Self::create_network_msg(*id, ModuleMessage::RequestEventIDs))
            .collect()
    }

    fn create_network_msg(dst: NodeID, msg: ModuleMessage) -> Vec<InternOut> {
        NetworkWrapper::wrap_yaml(MODULE_NAME, &msg)
            .map(|net_msg| {
                vec![InternOut::Network(RandomIn::NetworkWrapperToNetwork(
                    dst, net_msg,
                ))]
            })
            .unwrap_or(vec![])
    }
}

#[platform_async_trait()]
impl SubsystemHandler<InternIn, InternOut> for Intern {
    async fn messages(&mut self, msgs: Vec<InternIn>) -> Vec<InternOut> {
        msgs.into_iter()
            .flat_map(|msg| match msg {
                InternIn::Tick => self.msg_tick(),
                InternIn::Network(random_out) => self.msg_net(random_out),
                InternIn::Gossip(gossip_in) => self.msg_gossip(gossip_in),
            })
            .collect()
    }
}
