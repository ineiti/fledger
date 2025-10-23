use flarch::{
    add_translator_direct, add_translator_link, broker::Broker, data_storage::DataStorage,
    nodeids::U256, tasks::now,
};
use tokio::sync::watch;

use super::{
    core::{Category, Event, EventsStorage},
    intern::{Intern, InternIn, InternOut},
};
use crate::{
    nodeconfig::NodeInfo,
    random_connections::broker::BrokerRandom,
    timer::{BrokerTimer, Timer},
};

pub type BrokerGossip = Broker<GossipIn, GossipOut>;

pub(super) const MODULE_NAME: &str = "Gossip";

#[derive(Debug, Clone, PartialEq)]
pub enum GossipIn {
    AddEvent(Event),
}

#[derive(Debug, Clone, PartialEq)]
pub enum GossipOut {
    NewEvent(Event),
}

/// This links the GossipEvent module with a RandomConnections module, so that
/// all messages are correctly translated from one to the other.
#[derive(Clone, Debug)]
pub struct Gossip {
    /// Represents the underlying broker.
    pub broker: BrokerGossip,
    /// Is used to pass the EventsStorage structure from the Translate to the GossipLink.
    pub storage: watch::Receiver<EventsStorage>,
    pub info: NodeInfo,
}

impl Gossip {
    pub async fn start(
        storage: Box<dyn DataStorage + Send>,
        info: NodeInfo,
        timer: BrokerTimer,
        rc: BrokerRandom,
    ) -> anyhow::Result<Self> {
        let src = info.get_id();
        let (messages, storage) = Intern::new(src.clone(), storage);
        let mut intern = Broker::new_with_handler(Box::new(messages)).await?.0;

        Timer::second(timer, intern.clone(), InternIn::Tick).await?;
        add_translator_link!(intern, rc, InternIn::Network, InternOut::Network);

        let broker = Broker::new();
        add_translator_direct!(intern, broker.clone(), InternIn::Gossip, InternOut::Gossip);

        let mut gb = Gossip {
            storage,
            broker,
            info: info.clone(),
        };
        gb.add_event(Event {
            category: Category::NodeInfo,
            src,
            created: now(),
            msg: info.encode(),
        })
        .await?;

        Ok(gb)
    }

    pub async fn add_chat_message(&mut self, msg: String) -> anyhow::Result<()> {
        self.add_event(Event {
            category: Category::TextMessage,
            src: self.info.get_id(),
            created: now(),
            msg,
        })
        .await
    }

    /// Adds a new event to the GossipMessage module.
    /// The new event will automatically be propagated to all connected nodes.
    pub async fn add_event(&mut self, event: Event) -> anyhow::Result<()> {
        self.broker.emit_msg_in(GossipIn::AddEvent(event))?;
        Ok(())
    }

    /// Gets a copy of all chat events stored in the module.
    pub fn chat_events(&self) -> Vec<Event> {
        self.storage.borrow().events(Category::TextMessage)
    }

    /// Gets all event-ids that are stored in the module.
    pub fn event_ids(&self) -> Vec<U256> {
        self.storage.borrow().event_ids()
    }

    /// Gets a single event of the module.
    pub fn event(&self, id: &U256) -> Option<Event> {
        self.storage.borrow().event(id)
    }

    /// Returns all events from a given category
    pub fn events(&self, cat: Category) -> Vec<Event> {
        self.storage.borrow().events(cat)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use crate::gossip_events::core::{Category, Event};
    use crate::gossip_events::intern::ModuleMessage;
    use crate::nodeconfig::NodeConfig;
    use crate::random_connections::broker::{RandomIn, RandomOut};
    use crate::router::messages::NetworkWrapper;
    use crate::timer::TimerMessage;
    use flarch::data_storage::DataStorageTemp;
    use flarch::nodeids::NodeID;
    use flarch::start_logging_filter_level;
    use flarch::tasks::now;

    use super::*;

    #[tokio::test]
    async fn test_translation() -> anyhow::Result<()> {
        start_logging_filter_level(vec![], log::LevelFilter::Info);

        let node_info = NodeConfig::new().info;
        let mut broker_rnd = Broker::new();
        let mut timer = Timer::simul();
        let gossip = Gossip::start(
            Box::new(DataStorageTemp::new()),
            node_info,
            timer.broker.clone(),
            broker_rnd.clone(),
        )
        .await?;

        let id2 = NodeID::rnd();
        let (tap_rnd, _) = broker_rnd.get_tap_in_sync().await?;
        broker_rnd
            .settle_msg_out(RandomOut::NodeIDsConnected(vec![id2].into()))
            .await?;
        assert_msg_reid(&tap_rnd, &id2)?;
        // Drop the following list of eventIDs
        tap_rnd.recv()?;

        let event = Event {
            category: Category::TextMessage,
            src: id2,
            created: now(),
            msg: "test_msg".into(),
        };
        let msg = ModuleMessage::Events(vec![event.clone()]);
        broker_rnd
            .settle_msg_out(
                RandomOut::NetworkWrapperFromNetwork(
                    id2,
                    NetworkWrapper::wrap_yaml(MODULE_NAME, &msg).unwrap(),
                )
                .into(),
            )
            .await?;
        assert_eq!(1, gossip.storage.borrow().events(event.category).len());

        // Need three ticks to request new EventIDs
        timer.broker.settle_msg_out(TimerMessage::Second).await?;
        timer.broker.settle_msg_out(TimerMessage::Second).await?;
        timer.broker.settle_msg_out(TimerMessage::Second).await?;
        assert_msg_reid(&tap_rnd, &id2)?;
        Ok(())
    }

    fn assert_msg_reid(tap: &mpsc::Receiver<RandomIn>, id2: &NodeID) -> anyhow::Result<()> {
        let msg = tap.recv()?;
        log::info!("Message is: {msg:?}");
        if let RandomIn::NetworkWrapperToNetwork(id, msg_mod) = msg {
            assert_eq!(id2, &id);
            assert_eq!(MODULE_NAME.to_string(), msg_mod.module);
            let msg_yaml = serde_yaml::from_str(&msg_mod.msg)?;
            assert_eq!(ModuleMessage::RequestEventIDs, msg_yaml);
            Ok(())
        } else {
            anyhow::bail!("Message was {msg:?} instead of RequestEventIDs")
        }
    }
}
