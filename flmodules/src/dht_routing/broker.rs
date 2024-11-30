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
    use std::str::FromStr;

    use flarch::{start_logging_filter_level, tasks::wait_ms};

    use super::*;
    use crate::{nodeconfig::NodeInfo, overlay::testing::NetworkBrokerSimul};

    const LOG_LVL: log::LevelFilter = log::LevelFilter::Debug;

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
        let (nroot, mut root) = simul.new_node_id(Some(U256::from_str("00")?)).await?;
        let (n1, mut o1) = simul.new_node_id(Some(U256::from_str("80")?)).await?;
        let (n2, mut o2) = simul.new_node_id(Some(U256::from_str("81")?)).await?;
        let (n3, mut o3) = simul.new_node_id(Some(U256::from_str("40")?)).await?;
        let (n4, mut o4) = simul.new_node_id(Some(U256::from_str("41")?)).await?;
        let list: Vec<NodeInfo> = vec![&nroot, &n1, &n2, &n3, &n4]
            .iter()
            .map(|nc| nc.info.clone())
            .collect();

        let mut dhtr = DHTRouting::start(
            nroot.info.get_id(),
            root.clone(),
            tick.clone(),
            config.clone(),
        )
        .await?;
        let _dht1 =
            DHTRouting::start(n1.info.get_id(), o1.clone(), tick.clone(), config.clone()).await?;
        let _dht2 =
            DHTRouting::start(n2.info.get_id(), o2.clone(), tick.clone(), config.clone()).await?;
        let _dht3 =
            DHTRouting::start(n3.info.get_id(), o3.clone(), tick.clone(), config.clone()).await?;
        let _dht4 =
            DHTRouting::start(n4.info.get_id(), o4.clone(), tick.clone(), config.clone()).await?;
        root.emit_msg_out(OverlayOut::NodeInfoAvailable(list.clone()))?;
        o1.emit_msg_out(OverlayOut::NodeInfoAvailable(list.clone()))?;
        o2.emit_msg_out(OverlayOut::NodeInfoAvailable(list.clone()))?;
        o3.emit_msg_out(OverlayOut::NodeInfoAvailable(list.clone()))?;
        o4.emit_msg_out(OverlayOut::NodeInfoAvailable(list.clone()))?;

        wait_ms(1000).await;
        tick.emit_msg(TimerMessage::Second)?;
        wait_ms(1000).await;
        dhtr.routing.emit_msg_in(DHTRoutingIn::DHTMessage(
            list[4].get_id(),
            NetworkWrapper {
                module: "Test1".into(),
                msg: "Brume".into(),
            },
        ))?;
        wait_ms(1000).await;
        dhtr.routing.emit_msg_in(DHTRoutingIn::DHTMessage(
            U256::from_str("42")?,
            NetworkWrapper {
                module: "Test2".into(),
                msg: "Brume".into(),
            },
        ))?;
        wait_ms(1000).await;

        Ok(())
    }
}
