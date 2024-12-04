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
    MessageClosest(U256, NetworkWrapper),
    MessageDirect(NodeID, NetworkWrapper),
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
    use std::str::FromStr;

    use flarch::start_logging_filter_level;

    use super::*;
    use crate::overlay::testing::NetworkBrokerSimul;

    const LOG_LVL: log::LevelFilter = log::LevelFilter::Info;

    #[tokio::test]
    async fn test_routing() -> Result<(), Box<dyn Error>> {
        start_logging_filter_level(vec![], LOG_LVL);

        let mut tick = Broker::new();
        let config = Config {
            k: 1,
            ping_interval: 2,
            ping_timeout: 4,
        };

        let mut simul = NetworkBrokerSimul::new().await?;
        let mut node_ids = vec![];
        let mut node_infos = vec![];
        let mut overlay_nets = vec![];
        let mut dht_nets = vec![];
        let mut taps = vec![];
        for start in ["00", "40", "41", "42", "43"] {
            let (node_conf, brok) = simul.new_node_id(Some(U256::from_str(start)?)).await?;
            let id = node_conf.info.get_id();
            let mut dht = DHTRouting::start(id, brok.clone(), tick.clone(), config.clone()).await?;
            overlay_nets.push(brok);
            taps.push(dht.routing.get_tap_out().await?.0);
            dht_nets.push(dht.routing);
            node_ids.push(id);
            node_infos.push(node_conf.info);
        }

        for mut overlay in overlay_nets {
            overlay
                .settle_msg_out(OverlayOut::NodeInfoAvailable(node_infos.clone()))
                .await?;
        }

        tick.settle_msg(TimerMessage::Second).await?;

        let dhtm = |id: NodeID, module: &str, msg: &str| {
            DHTRoutingIn::MessageClosest(
                id,
                NetworkWrapper {
                    module: module.into(),
                    msg: msg.into(),
                },
            )
        };

        let mut nodes = node_ids.clone();
        nodes.push(U256::from_str("44")?);
        let mut routing = 0;
        for (i, node) in nodes.iter().enumerate() {
            if i == 0 {
                continue;
            }
            log::debug!("{i} Sending to node {node}");
            dht_nets[0]
                .settle_msg_in(dhtm(*node, "Test", &format!("Msg{i}")))
                .await?;
            let mut dest = 0;
            let mut closest = 0;
            for (j, tap) in taps.iter_mut().enumerate() {
                if let Ok(msg) = tap.try_recv() {
                    log::debug!("{j} got {msg:?}");
                    match msg {
                        DHTRoutingOut::MessageRouting(_, _, _, _, _) => routing += 1,
                        DHTRoutingOut::MessageClosest(_, _, _, _) => closest += 1,
                        DHTRoutingOut::MessageDest(_, _, _) => dest += 1,
                        _ => {}
                    }
                }
            }
            match i {
                5 => assert_eq!([0, 1], [dest, closest]),
                _ => assert_eq!([1, 0], [dest, closest]),
            }
        }
        assert!(routing > 0);

        Ok(())
    }
}
