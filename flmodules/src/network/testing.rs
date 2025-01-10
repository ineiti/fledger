use flarch::{
    broker::{Broker, BrokerError, Subsystem, SubsystemHandler, Translate},
    nodeids::U256,
    platform_async_trait,
};

use super::messages::{NetworkIn, NetworkOut, NetworkMessage};
use crate::nodeconfig::{NodeConfig, NodeInfo};

pub struct NetworkBrokerSimul {
    nsh_broker: Broker<NSHubMessage>,
}

impl NetworkBrokerSimul {
    pub async fn new() -> Result<Self, BrokerError> {
        let nsh_broker = NSHub::new().await?;
        Ok(Self { nsh_broker })
    }

    pub async fn new_node(&mut self) -> Result<(NodeConfig, Broker<NetworkMessage>), BrokerError> {
        let nc = NodeConfig::new();
        let nc_id = nc.info.get_id();
        let mut nm_broker = Broker::new();

        nm_broker
            .link_bi(
                self.nsh_broker.clone(),
                Self::nsh_net(nc_id.clone()),
                Self::net_nsh(nc_id),
            )
            .await?;
        self.nsh_broker
            .emit_msg(NSHubMessage::NewClient(nc.info.clone()))?;

        Ok((nc, nm_broker))
    }

    fn nsh_net(our_id: U256) -> Translate<NSHubMessage, NetworkMessage> {
        Box::new(move |msg| {
            if let NSHubMessage::ToClient(dst, net_msg) = msg {
                if dst == our_id {
                    return Some(net_msg);
                }
            }
            None
        })
    }

    fn net_nsh(our_id: U256) -> Translate<NetworkMessage, NSHubMessage> {
        Box::new(move |msg| Some(NSHubMessage::FromClient(our_id, msg)))
    }
}

#[derive(Clone, Debug, PartialEq)]
enum NSHubMessage {
    FromClient(U256, NetworkMessage),
    ToClient(U256, NetworkMessage),
    NewClient(NodeInfo),
}

struct NSHub {
    nodes: Vec<NodeInfo>,
}

impl NSHub {
    async fn new() -> Result<Broker<NSHubMessage>, BrokerError> {
        let mut b = Broker::new();
        b.add_subsystem(Subsystem::Handler(Box::new(Self { nodes: vec![] })))
            .await?;
        Ok(b)
    }

    fn net_msg(&self, id: U256, net_msg: NetworkMessage) -> Vec<NSHubMessage> {
        if let NetworkMessage::Input(msg) = net_msg {
            match msg {
                NetworkIn::MessageToNode(id_dst, msg_node) => {
                    vec![NSHubMessage::ToClient(
                        id_dst,
                        NetworkMessage::Output(NetworkOut::MessageFromNode(id, msg_node)),
                    )]
                }
                NetworkIn::WSUpdateListRequest => {
                    vec![NSHubMessage::ToClient(
                        id,
                        NetworkMessage::Output(NetworkOut::NodeListFromWS(self.nodes.clone())),
                    )]
                }
                _ => {
                    vec![]
                }
            }
        } else {
            vec![]
        }
    }
}

#[platform_async_trait()]
impl SubsystemHandler<NSHubMessage> for NSHub {
    async fn messages(&mut self, msgs: Vec<NSHubMessage>) -> Vec<NSHubMessage> {
        let mut out = vec![];

        for msg in msgs {
            match msg {
                NSHubMessage::FromClient(id, net_msg) => {
                    out.append(&mut self.net_msg(id, net_msg));
                }
                NSHubMessage::NewClient(info) => {
                    self.nodes.push(info);
                }
                _ => {}
            }
        }

        out.into_iter()
            .inspect(|msg| log::trace!("Sending message {:?}", msg))
            .map(|msg| msg)
            .collect()
    }
}
