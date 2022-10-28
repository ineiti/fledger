use std::error::Error;

use flarch::start_logging;
use flmodules::{
    gossip_events::{
        events::{Category, EventsStorage},
        module::{Config, GossipEvents, GossipIn, GossipOut, MessageNode},
    },
    nodeids::NodeID,
};

#[tokio::test]
// Makes sure that the messages from the old format are now correctly translated to the new format.
async fn test_gossip() -> Result<(), Box<dyn Error>> {
    start_logging();

    let evs = EventsConfig::new();

    let nc1 = NodeID::rnd();
    let mut es1 = GossipEvents::new(Config::new(nc1));
    es1.process_message(GossipIn::SetStorage(evs.storage1.clone()))?;

    let nc2 = NodeID::rnd();
    let mut es2 = GossipEvents::new(Config::new(nc2));
    es2.process_message(GossipIn::SetStorage(evs.storage2.clone()))?;

    let msg12 = es1.node_list(vec![nc1, nc2].into());
    let msg21 = es2.process_messages(msg_out(msg12))?;
    let msg12 = es1.process_messages(msg_out(msg21))?;
    let msg21 = es2.process_messages(msg_out(msg12))?;
    assert_eq!(1, msg21.len());
    if let GossipOut::Node(_, MessageNode::Events(events)) = &msg21[0] {
        assert_eq!(
            2,
            events
                .iter()
                .filter(|ev| ev.category == Category::TextMessage)
                .count()
        );
    } else {
        panic!("Didn't get node message with events");
    }

    Ok(())
}

fn msg_out(mut msgs: Vec<GossipOut>) -> Vec<GossipIn> {
    msgs.drain(..)
        .filter_map(|msg| match msg {
            GossipOut::Node(id, msg_node) => Some(GossipIn::Node(id, msg_node)),
            _ => None,
        })
        .collect()
}

struct EventsConfig {
    _config1: String,
    storage1: EventsStorage,
    _config2: String,
    storage2: EventsStorage,
}

impl EventsConfig {
    fn new() -> Self {
        let config1 = std::fs::read_to_string("tests/load/gossip_events-01.toml")
            .expect("Couldn't find gossip-events file");
        let mut storage1 = EventsStorage::new();
        storage1.set(&config1).expect("Setting storage");
        let config2 = std::fs::read_to_string("tests/load/gossip_events-02.toml")
            .expect("Couldn't find gossip-events file");
        let mut storage2 = EventsStorage::new();
        storage2.set(&config2).expect("Setting storage");
        Self {
            _config1: config1,
            storage1,
            _config2: config2,
            storage2,
        }
    }
}
