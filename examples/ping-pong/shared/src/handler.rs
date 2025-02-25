use flarch::{
    broker::{Broker, SubsystemHandler},
    nodeids::{NodeID, U256},
    platform_async_trait,
};
use flmodules::{
    network::broker::{BrokerNetwork, NetworkIn, NetworkOut},
    nodeconfig::NodeInfo,
    timer::Timer,
};

use crate::common::{PPMessageNode, PingPongIn, PingPongOut};

/// This only runs on localhost.
pub const URL: &str = "ws://localhost:8765";

/// The PingPong structure is a simple implementation showing how to use the flmodules::network
/// module and the Broker to create a request/reply system for multiple
/// nodes.
/// To use the PingPong structure, it can be called from a libc binary or a
/// wasm library.
pub struct PingPong {
    id: U256,
    nodes: Vec<NodeInfo>,
}

impl PingPong {
    /// Create a new broker for PPMessages that can be used to receive and send pings.
    /// Relevant messages from the network broker are forwarded to this broker and
    /// processed to interpret ping's and pong's.
    pub async fn new(
        id: U256,
        net: BrokerNetwork,
    ) -> anyhow::Result<Broker<PingPongIn, PingPongOut>> {
        let mut pp_broker = Broker::new();
        // Interpret network messages and wrap relevant messages to ourselves.
        pp_broker
            .add_translator_link(
                net.clone(),
                Box::new(|msg| {
                    Some(match msg {
                        PingPongOut::ToNetwork(dst, pp_msg) => {
                            NetworkIn::MessageToNode(dst, serde_json::to_string(&pp_msg).unwrap())
                        }
                        PingPongOut::WSUpdateListRequest => NetworkIn::WSUpdateListRequest,
                    })
                }),
                Box::new(Self::net_to_pp),
            )
            .await?;
        // Forward "second" ticks, but ignore minute ticks
        Timer::start()
            .await?
            .tick_second(pp_broker.clone(), PingPongIn::Tick)
            .await?;
        // Add ourselves as a handler to the broker.
        // Because of rust's ownership protection, this means that this structure is not available
        // anymore to the caller.
        // So the only way to interact with this structure is by sending messages to the broker.
        pp_broker
            .add_handler(Box::new(Self { id, nodes: vec![] }))
            .await
            .map(|_| ())?;
        Ok(pp_broker)
    }

    // Translates incoming messages from the network to messages that can be understood by PingPong.
    // The only two messages that need to be interpreted are MessageFromNode and NodeListFromWS.
    // For MessageFromNode, the enclosed message is interpreted as a PPMessageNode and sent to this
    // broker.
    // All other messages coming from Network are ignored.
    fn net_to_pp(msg: NetworkOut) -> Option<PingPongIn> {
        match msg {
            NetworkOut::MessageFromNode(from, node_msg) => {
                serde_json::from_str::<PPMessageNode>(&node_msg)
                    .ok()
                    .map(|ppm| PingPongIn::FromNetwork(from, ppm))
            }
            NetworkOut::NodeListFromWS(nodes) => Some(PingPongIn::List(nodes)),
            _ => None,
        }
    }
}

// Process messages sent to the PPMessage broker:
// ToNetwork messages are sent to the network, and automatically set up necessary connections.
// For PPMessageNode::Ping messages, a pong is replied, and an update list request is sent to
// the signalling server.
#[platform_async_trait()]
impl SubsystemHandler<PingPongIn, PingPongOut> for PingPong {
    async fn messages(&mut self, msgs: Vec<PingPongIn>) -> Vec<PingPongOut> {
        let mut out = vec![];
        for msg in msgs {
            log::trace!("{}: got message {:?}", self.id, msg);

            match msg {
                PingPongIn::FromNetwork(from, PPMessageNode::Ping) => {
                    out.push(PingPongOut::ToNetwork(from, PPMessageNode::Pong));
                    out.push(PingPongOut::WSUpdateListRequest);
                }
                PingPongIn::List(list) => self.nodes = list,
                PingPongIn::Tick => {
                    let nodes = self
                        .nodes
                        .iter()
                        .map(|n| n.get_id())
                        .filter(|n| *n != self.id)
                        .collect::<Vec<NodeID>>();
                    for node in nodes {
                        out.push(PingPongOut::ToNetwork(node, PPMessageNode::Ping));
                    }
                }
                _ => {}
            }
        }

        out
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use flarch::{broker::Destination, start_logging};
    use flmodules::network::broker::NetworkOut;
    use flmodules::nodeconfig::NodeConfig;
    use flmodules::testing::network_simul::{NetworkSimul, RouterNode};

    use super::*;

    // Tests single messages going into the structure doing the correct thing:
    // - receive 'ping' from the user, send a 'ping' to the network
    // - receive 'ping' from the network replies 'pong' and requests a new list
    // #BUG: flaky test - failed once with
    // thread 'handler::test::test_ping' panicked at src/handler.rs:190:9:
    // assertion `left == right` failed
    // left: FromNetwork(c52bea62dddd2f42-659253603d78279c-ea7f42654af68f7a-b5c8b6ad272a30ed, Ping)
    // right: Tick
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_ping() -> anyhow::Result<()> {
        start_logging();

        let nc_src = NodeConfig::new();
        let mut net = Broker::new();
        let (net_tap, _) = net.get_tap_in_sync().await?;
        let mut pp = PingPong::new(nc_src.info.get_id(), net.clone()).await?;
        let (pp_tap, _) = pp.get_tap_in_sync().await?;

        let nc_dst = NodeConfig::new();
        let dst_id = nc_dst.info.get_id();

        // Send a Ping message from src to dst
        pp.emit_msg_out_dest(
            Destination::NoTap,
            PingPongOut::ToNetwork(dst_id.clone(), PPMessageNode::Ping),
        )?;
        assert_eq!(
            node_msg(&dst_id, &PPMessageNode::Ping),
            net_tap.recv().unwrap()
        );

        // Remove the "Tick"
        pp_tap.recv().unwrap();

        // Receive a ping message through the network
        net.emit_msg_out_dest(
            Destination::NoTap,
            NetworkOut::MessageFromNode(
                dst_id.clone(),
                serde_json::to_string(&PPMessageNode::Ping).unwrap(),
            ),
        )?;
        assert_eq!(
            PingPongIn::FromNetwork(dst_id.clone(), PPMessageNode::Ping),
            pp_tap.recv().unwrap()
        );
        assert_eq!(
            node_msg(&dst_id, &PPMessageNode::Pong),
            net_tap.recv().unwrap()
        );
        assert_eq!(NetworkIn::WSUpdateListRequest, net_tap.recv().unwrap());

        Ok(())
    }

    fn node_msg(dst: &U256, msg: &PPMessageNode) -> NetworkIn {
        NetworkIn::MessageToNode(dst.clone(), serde_json::to_string(msg).unwrap())
    }

    use tokio::time::sleep;

    // Test a simulation of two nodes with the NetworkBrokerSimul
    // and run for 3 rounds.
    #[tokio::test]
    async fn test_network() -> anyhow::Result<()> {
        start_logging();

        let mut simul = NetworkSimul::new().await?;

        let RouterNode {
            config: nc1,
            net: net1,
            ..
        } = simul.new_node().await?;
        let mut pp1 = PingPong::new(nc1.info.get_id(), net1).await?;
        let (pp1_tap, _) = pp1.get_tap_out_sync().await?;
        log::info!("PingPong1 is: {}", nc1.info.get_id());

        let RouterNode {
            config: nc2,
            net: net2,
            ..
        } = simul.new_node().await?;
        let mut pp2 = PingPong::new(nc2.info.get_id(), net2).await?;
        let (pp2_tap, _) = pp2.get_tap_out_sync().await?;
        log::info!("PingPong2 is: {}", nc2.info.get_id());

        for _ in 0..3 {
            for msg in pp1_tap.try_iter() {
                log::info!("Got message from pp1: {:?}", msg);
            }
            for msg in pp2_tap.try_iter() {
                log::info!("Got message from pp2: {:?}", msg);
            }

            sleep(Duration::from_millis(1000)).await;

            pp1.emit_msg_out(PingPongOut::ToNetwork(
                nc2.info.get_id(),
                PPMessageNode::Ping,
            ))?;
            pp2.emit_msg_out(PingPongOut::ToNetwork(
                nc1.info.get_id(),
                PPMessageNode::Ping,
            ))?;
        }

        Ok(())
    }
}
