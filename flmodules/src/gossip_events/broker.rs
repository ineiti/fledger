use flarch::{
    broker::{Broker, TranslateFrom, TranslateInto},
    data_storage::DataStorage,
    nodeids::U256,
    tasks::now,
};
use tokio::sync::watch;

use super::{
    core::{Category, Event, EventsStorage},
    messages::{GossipIn, GossipOut, Messages},
};
use crate::{
    nodeconfig::NodeInfo,
    random_connections::broker::{BrokerRandom, RandomIn, RandomOut},
    router::messages::NetworkWrapper,
    timer::Timer,
};

pub type BrokerGossip = Broker<GossipIn, GossipOut>;

pub const MODULE_NAME: &str = "Gossip";

/// This links the GossipEvent module with a RandomConnections module, so that
/// all messages are correctly translated from one to the other.
pub struct Gossip {
    /// Represents the underlying broker.
    pub broker: BrokerGossip,
    /// Is used to pass the EventsStorage structure from the Translate to the GossipLink.
    pub storage: watch::Receiver<EventsStorage>,
}

impl Gossip {
    pub async fn start(
        storage: Box<dyn DataStorage + Send>,
        node_info: NodeInfo,
        rc: BrokerRandom,
        timer: &mut Timer,
    ) -> anyhow::Result<Self> {
        let (messages, storage) = Messages::new(node_info.get_id(), storage);
        let mut broker = Broker::new();
        broker.add_handler(Box::new(messages)).await?;

        timer.tick_minute(broker.clone(), GossipIn::Tick).await?;
        broker.link_bi(rc).await?;

        let mut gb = Gossip { storage, broker };
        gb.add_event(Event {
            category: Category::NodeInfo,
            src: node_info.get_id(),
            created: now(),
            msg: node_info.encode(),
        })
        .await?;

        Ok(gb)
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

impl TranslateFrom<RandomOut> for GossipIn {
    fn translate(msg: RandomOut) -> Option<Self> {
        match msg {
            RandomOut::NodeIDsConnected(list) => Some(GossipIn::UpdateNodeList(list.into())),
            RandomOut::NetworkWrapperFromNetwork(id, msg) => msg
                .unwrap_yaml(MODULE_NAME)
                .map(|msg| GossipIn::FromNetwork(id, msg)),
            _ => None,
        }
    }
}

impl TranslateInto<RandomIn> for GossipOut {
    fn translate(self) -> Option<RandomIn> {
        let GossipOut::ToNetwork(id, msg_node) = self;
        Some(RandomIn::NetworkWrapperToNetwork(
            id,
            NetworkWrapper::wrap_yaml(MODULE_NAME, &msg_node).unwrap(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use crate::gossip_events::core::{Category, Event};
    use crate::gossip_events::messages::ModuleMessage;
    use crate::nodeconfig::NodeConfig;
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
            broker_rnd.clone(),
            &mut timer,
        )
        .await?;

        let id2 = NodeID::rnd();
        let (tap_rnd, _) = broker_rnd.get_tap_in_sync().await?;
        broker_rnd
            .settle_msg_out(RandomOut::NodeIDsConnected(vec![id2].into()))
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
            .settle_msg_out(
                RandomOut::NetworkWrapperFromNetwork(
                    id2,
                    NetworkWrapper::wrap_yaml(MODULE_NAME, &msg).unwrap(),
                )
                .into(),
            )
            .await?;
        assert_eq!(1, gossip.storage.borrow().events(event.category).len());

        timer.broker.settle_msg_out(TimerMessage::Second).await?;
        assert_msg_reid(&tap_rnd, &id2)?;
        Ok(())
    }

    fn assert_msg_reid(tap: &mpsc::Receiver<RandomIn>, id2: &NodeID) -> anyhow::Result<()> {
        for msg in tap.try_iter() {
            if let RandomIn::NetworkWrapperToNetwork(id, msg_mod) = msg {
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
