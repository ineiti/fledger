use async_trait::async_trait;

use flmodules::{
    nodeids::{NodeID, U256},
    timer::{TimerBroker, TimerMessage},
};
use flnet::{
    broker::{Broker, BrokerError, Subsystem, SubsystemListener},
    network::{NetCall, NetworkMessage}, config::NodeInfo,
};

use crate::common::{PPMessage, PPMessageNode};

/// This only runs on localhost.
pub const URL: &str = "ws://localhost:8765";

/// The PingPong structure is a simple implementation showing how to use the flnet
/// module and the Broker to create a request/reply system for multiple
/// nodes.
/// To use the PingPong structure, it can be called from a libc binary or a
/// wasm library.
pub struct PingPong {
    id: U256,
    net: Broker<NetworkMessage>,
    nodes: Vec<NodeInfo>,
}

impl PingPong {
    /// Create a new broker for PPMessages that can be used to receive and send pings.
    /// Relevant messages from the network broker are forwarded to this broker and
    /// processed to interpret ping's and pong's.
    pub async fn new(
        id: U256,
        mut net: Broker<NetworkMessage>,
    ) -> Result<Broker<PPMessage>, BrokerError> {
        let mut pp_broker = Broker::new();
        // Interpret network messages and wrap relevant messages to ourselves.
        net.forward(pp_broker.clone(), Box::new(Self::net_to_pp))
            .await;
        // Forward "second" ticks, but ignore minute ticks
        TimerBroker::start()
            .await?
            .forward(
                pp_broker.clone(),
                Box::new(|msg| (msg == TimerMessage::Second).then(|| PPMessage::Tick)),
            )
            .await;
        // Add ourselves as a handler to the broker.
        // Because of rust's ownership protection, this means that this structure is not available
        // anymore to the caller.
        // So the only way to interact with this structure is by sending messages to the broker.
        pp_broker
            .add_subsystem(Subsystem::Handler(Box::new(Self {
                id,
                net,
                nodes: vec![],
            })))
            .await
            .map(|_| ())?;
        Ok(pp_broker)
    }

    // Translates incoming messages from the network to messages that can be understood by PingPong.
    // The only two messages that need to be interpreted are RcvNodeMessage and RcvWSUpdateList.
    // For RcvNodeMessage, the enclosed message is interpreted as a PPMessageNode and sent to this
    // broker.
    // All other messages coming from Network are ignored.
    fn net_to_pp(msg: NetworkMessage) -> Option<PPMessage> {
        if let NetworkMessage::Reply(rep) = msg {
            match rep {
                flnet::network::NetReply::RcvNodeMessage(from, node_msg) => {
                    serde_json::from_str::<PPMessageNode>(&node_msg)
                        .ok()
                        .map(|ppm| PPMessage::FromNetwork(from, ppm))
                }
                flnet::network::NetReply::RcvWSUpdateList(nodes) => {
                    Some(PPMessage::List(nodes))
                }
                _ => None,
            }
        } else {
            None
        }
    }

    // Sends a generic NetCall type message to the network-broker.
    async fn send_net(&mut self, msg: NetCall) {
        self.net
            .emit_msg(NetworkMessage::Call(msg.clone()))
            .err()
            .map(|e| log::error!("While sending {:?} to net: {:?}", msg, e));
    }

    // Wraps a PPMessageNode into a json and sends it over the network to the
    // dst address.
    async fn send_net_ppm(&mut self, dst: U256, msg: &PPMessageNode) {
        self.send_net(NetCall::SendNodeMessage(
            dst,
            serde_json::to_string(msg).unwrap(),
        ))
        .await;
    }
}

// Process messages sent to the PPMessage broker:
// ToNetwork messages are sent to the network, and automatically set up necessary connections.
// For PPMessageNode::Ping messages, a pong is replied, and an update list request is sent to
// the signalling server.
#[cfg_attr(feature = "nosend", async_trait(?Send))]
#[cfg_attr(not(feature = "nosend"), async_trait)]
impl SubsystemListener<PPMessage> for PingPong {
    async fn messages(&mut self, msgs: Vec<PPMessage>) -> Vec<PPMessage> {
        for msg in msgs {
            log::trace!("{}: got message {:?}", self.id, msg);

            match msg {
                PPMessage::ToNetwork(from, ppm) => {
                    self.send_net_ppm(from, &ppm).await;
                }
                PPMessage::FromNetwork(from, PPMessageNode::Ping) => {
                    self.send_net_ppm(from, &PPMessageNode::Pong).await;
                    self.send_net(NetCall::SendWSUpdateListRequest).await;
                }
                PPMessage::List(list) => self.nodes = list,
                PPMessage::Tick => {
                    let nodes = self
                        .nodes
                        .iter()
                        .map(|n| n.get_id())
                        .filter(|n| *n != self.id)
                        .collect::<Vec<NodeID>>();
                    for node in nodes {
                        self.send_net_ppm(node, &PPMessageNode::Ping).await;
                    }
                }
                _ => {}
            }
        }

        // All messages are sent directly to the network broker.
        vec![]
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use flarch::{start_logging, start_logging_filter_level};
    use flmodules::broker::Destination;
    use flnet::{config::NodeConfig, network::NetReply, NetworkSetupError};

    use super::*;

    // Tests single messages going into the structure doing the correct thing:
    // - receive 'ping' from the user, send a 'ping' to the network
    // - receive 'ping' from the network replies 'pong' and requests a new list
    #[tokio::test]
    async fn test_ping() -> Result<(), NetworkSetupError> {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let nc_src = NodeConfig::new();
        let mut net = Broker::new();
        let (net_tap, _) = net.get_tap().await?;
        let mut pp = PingPong::new(nc_src.info.get_id(), net.clone()).await?;
        let (pp_tap, _) = pp.get_tap().await?;

        let nc_dst = NodeConfig::new();
        let dst_id = nc_dst.info.get_id();

        // Send a Ping message from src to dst
        pp.emit_msg_dest(
            Destination::NoTap,
            PPMessage::ToNetwork(dst_id.clone(), PPMessageNode::Ping),
        )?;
        assert_eq!(
            node_msg(&dst_id, &PPMessageNode::Ping),
            net_tap.recv().unwrap()
        );

        // Receive a ping message through the network
        net.emit_msg_dest(
            Destination::NoTap,
            NetworkMessage::Reply(NetReply::RcvNodeMessage(
                dst_id.clone(),
                serde_json::to_string(&PPMessageNode::Ping).unwrap(),
            )),
        )?;
        assert_eq!(
            PPMessage::FromNetwork(dst_id.clone(), PPMessageNode::Ping),
            pp_tap.recv().unwrap()
        );
        assert_eq!(
            node_msg(&dst_id, &PPMessageNode::Pong),
            net_tap.recv().unwrap()
        );
        assert_eq!(
            NetworkMessage::Call(NetCall::SendWSUpdateListRequest),
            net_tap.recv().unwrap()
        );

        Ok(())
    }

    fn node_msg(dst: &U256, msg: &PPMessageNode) -> NetworkMessage {
        NetworkMessage::Call(NetCall::SendNodeMessage(
            dst.clone(),
            serde_json::to_string(msg).unwrap(),
        ))
    }

    use flnet::testing::NetworkBrokerSimul;
    use tokio::time::sleep;

    // Test a simulation of two nodes with the NetworkBrokerSimul
    // and run for 3 rounds.
    #[tokio::test]
    async fn test_network() -> Result<(), NetworkSetupError> {
        start_logging();

        let mut simul = NetworkBrokerSimul::new().await?;

        let (nc1, net1) = simul.new_node().await?;
        let mut pp1 = PingPong::new(nc1.info.get_id(), net1).await?;
        let (pp1_tap, _) = pp1.get_tap().await?;
        log::info!("PingPong1 is: {}", nc1.info.get_id());

        let (nc2, net2) = simul.new_node().await?;
        let mut pp2 = PingPong::new(nc2.info.get_id(), net2).await?;
        let (pp2_tap, _) = pp2.get_tap().await?;
        log::info!("PingPong2 is: {}", nc2.info.get_id());

        for _ in 0..3 {
            for msg in pp1_tap.try_iter() {
                log::info!("Got message from pp1: {:?}", msg);
            }
            for msg in pp2_tap.try_iter() {
                log::info!("Got message from pp2: {:?}", msg);
            }

            sleep(Duration::from_millis(1000)).await;

            pp1.emit_msg(PPMessage::ToNetwork(nc2.info.get_id(), PPMessageNode::Ping))?;
            pp2.emit_msg(PPMessage::ToNetwork(nc1.info.get_id(), PPMessageNode::Ping))?;
        }

        Ok(())
    }
}
