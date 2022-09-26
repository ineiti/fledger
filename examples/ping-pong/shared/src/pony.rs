use async_trait::async_trait;
use flmodules::{
    broker::{Broker, BrokerError, Subsystem, SubsystemListener},
    nodeids::NodeID,
};
use flnet::{
    config::NodeInfo,
    network::{NetCall, NetworkMessage},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq)]
pub enum PonyMessage {
    FromNode(NodeID, PonyMessageNet),
    PingNode(NodeID),
    NodeListRequest,
    NodeList(Vec<NodeInfo>),
}

impl PonyMessage {
    pub async fn new_broker(net: Broker<NetworkMessage>) -> Result<Broker<Self>, BrokerError> {
        let mut pony_broker = Broker::<PonyMessageBroker>::new();
        pony_broker
            .link_bi(
                net,
                Box::new(|msg| Some(PonyMessageBroker::Network(msg))),
                Box::new(|msg| msg.to_net()),
            )
            .await?;

        let pony = Broker::new();
        pony_broker
            .link_bi(
                pony.clone(),
                Box::new(|msg| Some(PonyMessageBroker::Pony(msg))),
                Box::new(|msg| msg.to_pony()),
            )
            .await?;
        pony_broker
            .add_subsystem(Subsystem::Handler(Box::new(Pony {})))
            .await?;
        Ok(pony)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum PonyMessageNet {
    Ping,
    Pong,
}

impl PonyMessageNet {
    fn to_net(self, to: NodeID) -> PonyMessageBroker {
        NetworkMessage::Call(NetCall::SendNodeMessage(
            to,
            serde_json::to_string(&self).unwrap(),
        ))
        .into()
    }
}

#[derive(Clone, Debug)]
enum PonyMessageBroker {
    Network(NetworkMessage),
    Pony(PonyMessage),
}

impl PonyMessageBroker {
    fn to_net(self) -> Option<NetworkMessage> {
        if let PonyMessageBroker::Network(net) = self {
            Some(net)
        } else {
            None
        }
    }
    fn to_pony(self) -> Option<PonyMessage> {
        if let PonyMessageBroker::Pony(pony) = self {
            Some(pony)
        } else {
            None
        }
    }
}

impl From<NetworkMessage> for PonyMessageBroker {
    fn from(msg: NetworkMessage) -> Self {
        PonyMessageBroker::Network(msg)
    }
}

impl From<PonyMessage> for PonyMessageBroker {
    fn from(msg: PonyMessage) -> Self {
        PonyMessageBroker::Pony(msg)
    }
}

struct Pony {}

#[cfg_attr(feature = "nosend", async_trait(?Send))]
#[cfg_attr(not(feature = "nosend"), async_trait)]
impl SubsystemListener<PonyMessageBroker> for Pony {
    async fn messages(
        &mut self,
        msgs: Vec<PonyMessageBroker>,
    ) -> Vec<PonyMessageBroker> {
        let mut out = vec![];

        for msg in msgs {
            match msg {
                PonyMessageBroker::Network(net) => {
                    if let NetworkMessage::Reply(nr) = net {
                        match nr {
                            flnet::network::NetReply::RcvNodeMessage(from, msg) => {
                                if let Ok(pony_msg) = serde_json::from_str(&msg) {
                                    if pony_msg == PonyMessageNet::Ping {
                                        out.push(PonyMessageNet::Pong.to_net(from));
                                    }
                                    out.push(PonyMessage::FromNode(from, pony_msg).into());
                                }
                            }
                            flnet::network::NetReply::RcvWSUpdateList(list) => {
                                out.push(PonyMessage::NodeList(list).into())
                            }
                            _ => {}
                        }
                    }
                }
                PonyMessageBroker::Pony(pony) => match pony {
                    PonyMessage::PingNode(id) => {
                        out.push(PonyMessageNet::Ping.to_net(id));
                    }
                    PonyMessage::NodeListRequest => {
                        out.push(NetworkMessage::Call(NetCall::SendWSUpdateListRequest).into());
                    }
                    _ => {}
                },
            }
        }

        out
    }
}

#[cfg(test)]
mod test {
    use flarch::start_logging;
    use flmodules::broker::Destination;
    use flnet::{network::NetReply, NetworkSetupError};

    use super::*;

    #[tokio::test]
    async fn test_ping() -> Result<(), NetworkSetupError> {
        start_logging();

        let mut net = Broker::new();
        let (net_tap, _) = net.get_tap().await?;
        let mut ping_pong = PonyMessage::new_broker(net.clone()).await?;
        let (pp_tap, _) = ping_pong.get_tap().await?;

        ping_pong
            .emit_msg_dest(Destination::NoTap, PonyMessage::NodeListRequest)
            .await?;
        assert!(pp_tap.try_recv().is_err());
        assert_eq!(
            NetworkMessage::Call(NetCall::SendWSUpdateListRequest),
            net_tap.try_recv().unwrap()
        );

        let id = NodeID::rnd();
        net.emit_msg_dest(
            Destination::NoTap,
            NetworkMessage::Reply(NetReply::RcvNodeMessage(
                id,
                serde_json::to_string(&PonyMessageNet::Ping).unwrap(),
            )),
        )
        .await?;
        assert_eq!(
            PonyMessage::FromNode(id, PonyMessageNet::Ping),
            pp_tap.recv().unwrap()
        );

        Ok(())
    }
}
