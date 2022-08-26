use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use flmodules::nodeids::U256;
use flnet::{
    broker::{Broker, BrokerError, Destination, Subsystem, SubsystemListener},
    network::{NetCall, NetworkMessage},
};

pub const URL: &str = "ws://localhost:8765";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum PPMessage {
    ToNetwork(U256, PPMessageNode),
    FromNetwork(U256, PPMessageNode),
    List(Vec<U256>),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum PPMessageNode {
    Ping,
    Pong,
}

pub struct PingPong {
    id: U256,
    net: Broker<NetworkMessage>,
}

impl PingPong {
    pub async fn new(
        id: U256,
        mut net: Broker<NetworkMessage>,
    ) -> Result<Broker<PPMessage>, BrokerError> {
        // Create a broker for PPMessages and add this structure to it.
        // Additionally, filter messages from the network broker before
        // forwarding them to this broker.

        let mut pp_broker = Broker::new();
        net.forward(pp_broker.clone(), Box::new(Self::from_net))
            .await;
        pp_broker
            .add_subsystem(Subsystem::Handler(Box::new(Self { id, net })))
            .await
            .map(|_| ())?;
        Ok(pp_broker)
    }

    // Translates incoming messages from the network to messages that can be understood by PingPong.
    pub fn from_net(msg: NetworkMessage) -> Option<PPMessage> {
        if let NetworkMessage::Reply(rep) = msg {
            match rep {
                flnet::network::NetReply::RcvNodeMessage((from, node_msg)) => {
                    serde_json::from_str::<PPMessageNode>(&node_msg)
                        .ok()
                        .map(|ppm| PPMessage::FromNetwork(from, ppm))
                }
                flnet::network::NetReply::RcvWSUpdateList(nodes) => {
                    Some(PPMessage::List(nodes.iter().map(|n| n.get_id()).collect()))
                }
                _ => None,
            }
        } else {
            None
        }
    }

    pub async fn send_net(&mut self, msg: NetCall) {
        self.net
            .enqueue_msg(NetworkMessage::Call(msg.clone()))
            .await
            .err()
            .map(|e| log::error!("While sending {:?} to net: {:?}", msg, e));
    }

    pub async fn send_net_ppm(&mut self, id: U256, msg: &PPMessageNode) {
        self.send_net(NetCall::SendNodeMessage((
            id,
            serde_json::to_string(msg).unwrap(),
        )))
        .await;
    }
}

#[cfg_attr(feature = "nosend", async_trait(?Send))]
#[cfg_attr(not(feature = "nosend"), async_trait)]
impl SubsystemListener<PPMessage> for PingPong {
    async fn messages(&mut self, msgs: Vec<PPMessage>) -> Vec<(Destination, PPMessage)> {
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
                _ => {}
            }
        }
        self.net.process().await.err();
        vec![]
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use flarch::start_logging;
    use flnet::{config::NodeConfig, network::NetReply, NetworkSetupError};

    use super::*;

    #[tokio::test]
    async fn test_ping() -> Result<(), NetworkSetupError> {
        start_logging();

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
        )
        .await?;
        assert_eq!(
            node_msg(&dst_id, &PPMessageNode::Ping),
            net_tap.recv().unwrap()
        );

        // Receive a ping message through the network
        net.emit_msg_dest(
            Destination::NoTap,
            NetworkMessage::Reply(NetReply::RcvNodeMessage((
                dst_id.clone(),
                serde_json::to_string(&PPMessageNode::Ping).unwrap(),
            ))),
        )
        .await?;
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
        NetworkMessage::Call(NetCall::SendNodeMessage((
            dst.clone(),
            serde_json::to_string(msg).unwrap(),
        )))
    }

    use flnet::testing::NetworkBrokerSimul;
    use tokio::time::sleep;

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
            log::info!("");

            pp1.emit_msg(PPMessage::ToNetwork(nc2.info.get_id(), PPMessageNode::Ping)).await?;
            pp2.emit_msg(PPMessage::ToNetwork(nc1.info.get_id(), PPMessageNode::Ping)).await?;
        }

        Ok(())
    }
}
