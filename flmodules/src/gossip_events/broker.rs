use std::sync::mpsc::{channel, Receiver, Sender};

use flarch::{
    broker::{Broker, BrokerError, Subsystem, SubsystemHandler},
    nodeids::{NodeID, U256},
    platform_async_trait,
};

use super::{
    core::{Category, Event, EventsStorage},
    messages::{Config, GossipEvents, GossipIn, GossipMessage, GossipOut},
};
use crate::{
    overlay::messages::NetworkWrapper,
    random_connections::messages::{RandomIn, RandomMessage, RandomOut},
    timer::TimerMessage,
};

const MODULE_NAME: &str = "Gossip";

/// This links the GossipEvent module with a RandomConnections module, so that
/// all messages are correctly translated from one to the other.
pub struct GossipBroker {
    /// This is always updated with the latest view of the GossipEvent module.
    pub storage: EventsStorage,
    /// Represents the underlying broker.
    pub broker: Broker<GossipMessage>,
    /// Is used to pass the EventsStorage structure from the Translate to the GossipLink.
    storage_rx: Receiver<EventsStorage>,
}

impl GossipBroker {
    pub async fn start(id: NodeID, rc: Broker<RandomMessage>) -> Result<Self, BrokerError> {
        let (storage_tx, storage_rx) = channel();
        let broker = Translate::start(rc, Config::new(id), storage_tx).await?;
        Ok(GossipBroker {
            storage: EventsStorage::new(),
            storage_rx,
            broker,
        })
    }

    pub async fn add_timer(&mut self, mut timer: Broker<TimerMessage>) {
        timer
            .forward(
                self.broker.clone(),
                Box::new(|msg: TimerMessage| {
                    matches!(msg, TimerMessage::Minute)
                        .then(|| GossipMessage::Input(GossipIn::Tick))
                }),
            )
            .await;
    }

    pub fn update(&mut self) {
        for update in self.storage_rx.try_iter() {
            self.storage = update;
        }
    }

    /// Adds a new event to the GossipMessage module.
    /// The new event will automatically be propagated to all connected nodes.
    pub async fn add_event(&mut self, event: Event) -> Result<(), BrokerError> {
        self.broker
            .emit_msg(GossipMessage::Input(GossipIn::AddEvent(event)))?;
        Ok(())
    }

    /// Gets a copy of all chat events stored in the module.
    pub fn chat_events(&self) -> Vec<Event> {
        self.storage.events(Category::TextMessage)
    }

    /// Gets all event-ids that are stored in the module.
    pub fn event_ids(&self) -> Vec<U256> {
        self.storage.event_ids()
    }

    /// Gets a single event of the module.
    pub fn event(&self, id: &U256) -> Option<Event> {
        self.storage.event(id)
    }

    /// Returns all events from a given category
    pub fn events(&self, cat: Category) -> Vec<Event> {
        self.storage.events(cat)
    }
}

struct Translate {
    storage_tx: Sender<EventsStorage>,
    module: GossipEvents,
}

impl Translate {
    async fn start(
        random: Broker<RandomMessage>,
        config: Config,
        storage_tx: Sender<EventsStorage>,
    ) -> Result<Broker<GossipMessage>, BrokerError> {
        let mut gossip = Broker::new();
        gossip
            .add_subsystem(Subsystem::Handler(Box::new(Translate {
                storage_tx,
                module: GossipEvents::new(config),
            })))
            .await?;
        gossip
            .link_bi(
                random,
                Box::new(Self::link_rnd_gossip),
                Box::new(Self::link_gossip_rnd),
            )
            .await?;
        Ok(gossip)
    }

    fn link_rnd_gossip(msg: RandomMessage) -> Option<GossipMessage> {
        if let RandomMessage::Output(msg_out) = msg {
            match msg_out {
                RandomOut::NodeIDsConnected(list) => Some(GossipIn::NodeList(list.into()).into()),
                RandomOut::NetworkWrapperFromNetwork(id, msg) => msg
                    .unwrap_yaml(MODULE_NAME)
                    .map(|msg| GossipIn::FromNetwork(id, msg).into()),
                _ => None,
            }
        } else {
            None
        }
    }

    fn link_gossip_rnd(msg: GossipMessage) -> Option<RandomMessage> {
        if let GossipMessage::Output(GossipOut::ToNetwork(id, msg_node)) = msg {
            Some(
                RandomIn::NetworkWrapperToNetwork(
                    id,
                    NetworkWrapper::wrap_yaml(MODULE_NAME, &msg_node).unwrap(),
                )
                .into(),
            )
        } else {
            None
        }
    }

    fn handle_input(&mut self, msg_in: GossipIn) -> Vec<GossipOut> {
        match self.module.process_message(msg_in) {
            Ok(ret) => return ret.into_iter().map(|m| m.into()).collect(),
            Err(e) => log::warn!("While processing message: {e:?}"),
        }
        vec![]
    }

    fn handle_output(&mut self, msg_out: &GossipOut) {
        if let GossipOut::Storage(s) = msg_out {
            self.storage_tx
                .send(s.clone())
                .err()
                .map(|e| log::error!("Couldn't send to storage_tx: {e:?}"));
        }
    }
}

#[platform_async_trait()]
impl SubsystemHandler<GossipMessage> for Translate {
    async fn messages(&mut self, msgs: Vec<GossipMessage>) -> Vec<GossipMessage> {
        let mut out = vec![];
        for msg in msgs {
            log::trace!("Got msg: {msg:?}");
            if let GossipMessage::Input(msg_in) = msg {
                out.extend(self.handle_input(msg_in));
            }
        }
        for msg in out.iter() {
            log::trace!("Outputting: {msg:?}");
            self.handle_output(msg);
        }
        out.into_iter().map(|o| o.into()).collect()
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use crate::gossip_events::core::{Category, Event};
    use crate::gossip_events::messages::ModuleMessage;
    use flarch::nodeids::NodeID;
    use flarch::{start_logging, tasks::now};

    use super::*;

    #[tokio::test]
    async fn test_translation() -> Result<(), Box<dyn Error>> {
        start_logging();

        let id = NodeID::rnd();
        let mut broker_rnd = Broker::new();
        let mut gossip = GossipBroker::start(id, broker_rnd.clone()).await?;

        let id2 = NodeID::rnd();
        let (tap_rnd, _) = broker_rnd.get_tap_sync().await?;
        broker_rnd
            .settle_msg(RandomMessage::Output(RandomOut::NodeIDsConnected(
                vec![id2].into(),
            )))
            .await?;
        assert_msg_reid(&tap_rnd, &id2)?;

        let event = Event {
            category: Category::TextMessage,
            src: id2,
            created: now(),
            msg: "test_msg".into(),
        };
        let msg = ModuleMessage::Events(vec![event.clone()]);
        broker_rnd
            .settle_msg(
                RandomOut::NetworkWrapperFromNetwork(
                    id2,
                    NetworkWrapper::wrap_yaml(MODULE_NAME, &msg).unwrap(),
                )
                .into(),
            )
            .await?;
        gossip.update();
        assert_eq!(1, gossip.storage.events(event.category).len());

        gossip.broker.settle_msg(GossipIn::Tick.into()).await?;
        assert_msg_reid(&tap_rnd, &id2)?;
        Ok(())
    }

    fn assert_msg_reid(tap: &Receiver<RandomMessage>, id2: &NodeID) -> Result<(), Box<dyn Error>> {
        for msg in tap.try_iter() {
            if let RandomMessage::Input(RandomIn::NetworkWrapperToNetwork(id, msg_mod)) = msg {
                assert_eq!(id2, &id);
                assert_eq!(MODULE_NAME.to_string(), msg_mod.module);
                let msg_yaml = serde_yaml::from_str(&msg_mod.msg)?;
                assert_eq!(ModuleMessage::RequestEventIDs, msg_yaml);
            } else {
                assert!(false);
            }
        }
        Ok(())
    }
}
