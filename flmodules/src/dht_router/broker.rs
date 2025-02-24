use flarch::{
    broker::{Broker, BrokerError},
    nodeids::U256,
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::{
    router::{broker::BrokerRouter, messages::NetworkWrapper},
    timer::Timer,
};
use flarch::nodeids::NodeID;

use super::{
    kademlia::Config,
    messages::{Messages, InternIn, InternOut, Stats},
};

pub(super) const MODULE_NAME: &str = "DHTRouter";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DHTRouterIn {
    /// Sends a message to all nodes connected directly to this node.
    MessageBroadcast(NetworkWrapper),
    /// Routes the message to the closest node representing the ID.
    MessageClosest(U256, NetworkWrapper),
    /// Routes the message to a specific node. No guarantee can be given
    /// as to the delivery of the message.
    MessageDirect(NodeID, NetworkWrapper),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DHTRouterOut {
    MessageBroadcast(NodeID, NetworkWrapper),
    // MessageRouting(origin, last_hop, next_hop, key, msg)
    MessageRouting(NodeID, NodeID, NodeID, U256, NetworkWrapper),
    // MessageClosest(origin, last_hop, key, msg)
    MessageClosest(NodeID, NodeID, U256, NetworkWrapper),
    // MessageDest(origin, last_hop, msg)
    MessageDest(NodeID, NodeID, NetworkWrapper),
    NodeList(Vec<NodeID>),
}

pub type BrokerDHTRouter = Broker<DHTRouterIn, DHTRouterOut>;

/// This links the DHTRouter module with other modules, so that
/// all messages are correctly translated from one to the other.
/// For this example, it uses the RandomConnections module to communicate
/// with other nodes.
#[derive(Clone)]
pub struct DHTRouter {
    pub broker: BrokerDHTRouter,
    pub stats: watch::Receiver<Stats>,
}

impl DHTRouter {
    pub async fn start(
        our_id: NodeID,
        router: BrokerRouter,
        tick: &mut Timer,
        config: Config,
    ) -> Result<Self, BrokerError> {
        let (messages, stats) = Messages::new(our_id, config);
        let mut intern = Broker::new();
        intern.add_handler(Box::new(messages)).await?;
        intern
            .add_translator_link(
                router,
                Box::new(|msg| match msg {
                    InternOut::Network(to_router) => Some(to_router),
                    _ => None,
                }),
                Box::new(|msg| Some(InternIn::Network(msg))),
            )
            .await?;
        tick.tick_second(intern.clone(), InternIn::Tick).await?;

        let broker = Broker::new();
        intern
            .add_translator_direct(
                broker.clone(),
                Box::new(|msg| Some(InternIn::DHTRouter(msg))),
                Box::new(|msg| match msg {
                    InternOut::DHTRouter(dht) => Some(dht),
                    _ => None,
                }),
            )
            .await?;

        Ok(DHTRouter { broker, stats })
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error, str::FromStr};

    use flarch::{start_logging_filter_level, tasks::wait_ms};

    use crate::{
        router::messages::RouterOut, testing::router_simul::RouterSimul, timer::TimerMessage,
    };

    use super::*;

    const LOG_LVL: log::LevelFilter = log::LevelFilter::Info;

    #[tokio::test]
    async fn test_routing() -> Result<(), Box<dyn Error>> {
        start_logging_filter_level(vec![], LOG_LVL);

        let mut tick = Timer::simul();
        let config = Config {
            k: 1,
            ping_interval: 2,
            ping_timeout: 4,
        };

        let mut simul = RouterSimul::new().await?;
        let mut node_ids = vec![];
        let mut node_infos = vec![];
        let mut router_nets = vec![];
        let mut dht_nets = vec![];
        let mut dhts = vec![];
        let mut taps = vec![];
        for start in ["00", "40", "41", "42", "43"] {
            let (node_conf, brok) = simul.new_node_id(Some(U256::from_str(start)?)).await?;
            let id = node_conf.info.get_id();
            let mut dht = DHTRouter::start(id, brok.clone(), &mut tick, config.clone()).await?;
            router_nets.push(brok);
            taps.push(dht.broker.get_tap_out().await?.0);
            dht_nets.push(dht.broker.clone());
            dhts.push(dht);
            node_ids.push(id);
            node_infos.push(node_conf.info);
        }

        log::info!("After for");
        
        for mut net in router_nets {
            net.settle_msg_out(RouterOut::NodeInfoAvailable(node_infos.clone()))
                .await?;
        }

        tick.broker.settle_msg_out(TimerMessage::Second).await?;

        let dhtm_closest = |id: NodeID, module: &str, msg: &str| {
            DHTRouterIn::MessageClosest(
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
            log::info!("{i} Sending to node {node}");
            dht_nets[0]
                .settle_msg_in(dhtm_closest(*node, "Test", &format!("Msg{i}")))
                .await?;
            for b in &mut dht_nets {
                b.settle(vec![]).await?;
            }
            let mut dest = 0;
            let mut closest = 0;
            for (j, tap) in taps.iter_mut().enumerate() {
                while let Ok(msg) = tap.try_recv() {
                    log::debug!("{j} got {msg:?}");
                    match msg {
                        DHTRouterOut::MessageRouting(_, _, _, _, _) => routing += 1,
                        DHTRouterOut::MessageClosest(_, _, _, _) => closest += 1,
                        DHTRouterOut::MessageDest(_, _, _) => dest += 1,
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

    #[tokio::test]
    async fn test_update_nodes() -> Result<(), Box<dyn Error>> {
        start_logging_filter_level(vec!["flmodules"], log::LevelFilter::Info);

        let mut tick = Timer::simul();
        let config = Config {
            k: 1,
            ping_interval: 2,
            ping_timeout: 4,
        };

        let mut simul = RouterSimul::new().await?;
        let mut node_ids = vec![];
        let mut node_infos = vec![];
        let mut router_nets = vec![];
        let mut dht_nets = vec![];
        let mut taps = vec![];
        for start in ["00", "40", "41"] {
            let (node_conf, brok) = simul.new_node_id(Some(U256::from_str(start)?)).await?;
            let id = node_conf.info.get_id();
            let mut dht = DHTRouter::start(id, brok.clone(), &mut tick, config).await?;
            router_nets.push(brok);
            taps.push(dht.broker.get_tap_out().await?.0);
            dht_nets.push(dht.broker.clone());
            node_ids.push(id);
            node_infos.push(node_conf.info);
        }
        wait_ms(1000).await;

        Ok(())
    }
}
