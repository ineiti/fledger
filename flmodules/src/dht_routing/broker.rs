use flarch::{broker::Broker, broker_io::BrokerIO, nodeids::U256};
use serde::{Deserialize, Serialize};
use std::error::Error;

use crate::{
    overlay::messages::{NetworkWrapper, OverlayIn, OverlayOut},
    timer::TimerMessage,
};
use flarch::nodeids::NodeID;

use super::{
    kademlia::Config,
    messages::{DHTRoutingMessages, InternIn, InternOut},
};

pub(super) const MODULE_NAME: &str = "DHTRouting";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DHTRoutingIn {
    DHTMessage(U256, NetworkWrapper),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DHTRoutingOut {
    // MessageRouting(origin, last_hop, next_hop, key, msg)
    MessageRouting(NodeID, NodeID, NodeID, U256, NetworkWrapper),
    // MessageClosest(origin, last_hop, key, msg)
    MessageClosest(NodeID, NodeID, U256, NetworkWrapper),
    // MessageDest(origin, last_hop, msg)
    MessageDest(NodeID, NodeID, NetworkWrapper),
    Stats(DHTRoutingStats),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DHTRoutingStats {
    pub nodes_active: usize,
}

/// This links the DHTRouting module with other modules, so that
/// all messages are correctly translated from one to the other.
/// For this example, it uses the RandomConnections module to communicate
/// with other nodes.
///
/// The [DHTRouting] holds the [Translate] and offers convenience methods
/// to interact with [Translate] and [DHTRoutingMessage].
pub struct DHTRouting {
    pub routing: BrokerIO<DHTRoutingIn, DHTRoutingOut>,
}

impl DHTRouting {
    pub async fn start(
        our_id: NodeID,
        net: BrokerIO<OverlayIn, OverlayOut>,
        tick: Broker<TimerMessage>,
        config: Config,
    ) -> Result<Self, Box<dyn Error>> {
        let messages = DHTRoutingMessages::new(our_id, config);
        let mut dht_routing = BrokerIO::<InternIn, InternOut>::new();
        let routing = BrokerIO::new();

        dht_routing.add_handler(Box::new(messages)).await?;
        dht_routing
            .add_translator_link(
                net.clone(),
                Box::new(|msg| match msg {
                    InternOut::Network(overlay) => Some(overlay),
                    _ => None,
                }),
                Box::new(|msg| Some(InternIn::Network(msg))),
            )
            .await?;
        dht_routing
            .add_translator_direct(
                routing.clone(),
                Box::new(|msg| Some(InternIn::DHTRouting(msg))),
                Box::new(|msg| match msg {
                    InternOut::DHTRouting(dht) => Some(dht),
                    _ => None,
                }),
            )
            .await?;
        dht_routing.link_broker(tick).await?;

        Ok(DHTRouting { routing })
    }
}

#[cfg(test)]
mod tests {
    use flarch::start_logging_filter_level;

    use super::*;

    #[tokio::test]
    async fn test_increase() -> Result<(), Box<dyn Error>> {
        start_logging_filter_level(vec![], log::LevelFilter::Info);

        // let ds = Box::new(DataStorageTemp::new());
        // let id0 = NodeID::rnd();
        // let id1 = NodeID::rnd();
        // let mut rnd = Broker::new();
        // let mut tr = DHTRouting::start(ds, id0, rnd.clone(), DHTRoutingConfig::default()).await?;
        // let mut tap = rnd.get_tap().await?;
        // assert_eq!(0, tr.get_counter());

        // rnd.settle_msg(RandomMessage::Output(RandomOut::NodeIDsConnected(
        //     vec![id1].into(),
        // )))
        // .await?;
        // tr.increase_self(1)?;
        // assert!(matches!(
        //     tap.0.recv().await.unwrap(),
        //     RandomMessage::Input(_)
        // ));
        // assert_eq!(1, tr.get_counter());
        Ok(())
    }
}
