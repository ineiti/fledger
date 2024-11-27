use flarch::{broker_io::BrokerIO, nodeids::U256};
use serde::{Deserialize, Serialize};
use std::error::Error;

use crate::overlay::messages::{NetworkWrapper, OverlayIn, OverlayOut};
use flarch::nodeids::NodeID;

use super::{
    kademlia::Config,
    messages::{DHTRoutingMessages, InternIn, InternOut, ModuleMessage},
};

pub(super) const MODULE_NAME: &str = "DHTRouting";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DHTRoutingMessage {
    // Send the NetworkWrapper message to the closest node of U256.
    // Usually the destination doesn't exist, so whenever there is no
    // closer node to send the message to, propagation stops.
    Request(NodeID, U256, NetworkWrapper),
    // The destination of the reply
    Reply(NodeID, U256, NetworkWrapper),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DHTRoutingIn {
    Message(DHTRoutingMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DHTRoutingOut {
    // Reply(origin, last_hop, key, msg)
    Reply(NodeID, NodeID, U256, NetworkWrapper),
    // ReplyRouting(origin, last_hop, next_hop, destination, key, msg)
    ReplyRouting(NodeID, NodeID, NodeID, NodeID, U256, NetworkWrapper),
    // RequestClosest(origin, last_hop, key, msg)
    RequestClosest(NodeID, NodeID, U256, NetworkWrapper),
    // RequestRouting(origin, last_hop, next_hop, key, msg)
    RequestRouting(NodeID, NodeID, NodeID, U256, NetworkWrapper),
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

        Ok(DHTRouting { routing })
    }
}

impl DHTRoutingMessage {
    pub fn wrapper_network(self, dst: NodeID) -> InternOut {
        ModuleMessage::DHT(self).wrapper_network(dst)
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
