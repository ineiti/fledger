use flarch::{
    broker::{Broker, BrokerError, SubsystemHandler},
    nodeids::{NodeID, U256},
    platform_async_trait,
};

use crate::{
    network::broker::{BrokerNetwork, NetworkIn, NetworkOut},
    nodeconfig::{NodeConfig, NodeInfo},
    router::broker::{BrokerRouter, RouterNetwork},
};

#[derive(Clone)]
pub struct RouterNode {
    pub config: NodeConfig,
    pub net: BrokerNetwork,
    pub router: BrokerRouter,
}

pub struct NetworkSimul {
    pub nodes: Vec<RouterNode>,
    nsh_broker: Broker<NSHubIn, NSHubOut>,
}

impl NetworkSimul {
    pub async fn new() -> Result<Self, BrokerError> {
        let nsh_broker = NSHub::new().await?;
        Ok(Self {
            nsh_broker,
            nodes: vec![],
        })
    }

    pub async fn new_node_broker(&mut self) -> Result<(NodeConfig, BrokerNetwork), BrokerError> {
        let nc = NodeConfig::new();
        let nc_id = nc.info.get_id();
        let nm_broker = Broker::new();

        self.nsh_broker
            .add_translator_direct(
                nm_broker.clone(),
                Box::new(move |msg| Some(NSHubIn::FromClient(nc_id, msg))),
                Box::new(move |msg| {
                    let NSHubOut::ToClient(dst, net_msg) = msg;
                    (dst == nc_id).then_some(net_msg)
                }),
            )
            .await?;
        self.nsh_broker
            .emit_msg_in(NSHubIn::NewClient(nc.info.clone()))?;

        Ok((nc, nm_broker))
    }

    pub async fn new_node(&mut self) -> Result<RouterNode, BrokerError> {
        let (config, net) = self.new_node_broker().await?;
        let node = RouterNode {
            config,
            net: net.clone(),
            router: RouterNetwork::start(net).await?,
        };
        self.nodes.push(node.clone());
        self.settle().await?;
        Ok(node)
    }

    pub async fn settle(&mut self) -> Result<(), BrokerError> {
        self.nsh_broker.settle(vec![]).await
    }

    pub fn node_ids(&self) -> Vec<NodeID> {
        self.nodes.iter().map(|n| n.config.info.get_id()).collect()
    }

    pub async fn send_node_info(&mut self) -> Result<(), BrokerError> {
        let infos: Vec<NodeInfo> = self.nodes.iter().map(|n| n.config.info.clone()).collect();
        for node in &mut self.nodes {
            let our_id = node.config.info.get_id();
            let our_infos: Vec<NodeInfo> = infos
                .iter()
                .filter(|&info| info.get_id() != our_id)
                .cloned()
                .collect();
            self.nsh_broker.emit_msg_out(NSHubOut::ToClient(
                our_id,
                NetworkOut::NodeListFromWS(our_infos),
            ))?;
            self.nsh_broker.settle(vec![]).await?;
        }
        self.settle().await
    }
}

#[derive(Clone, Debug, PartialEq)]
enum NSHubIn {
    FromClient(U256, NetworkIn),
    NewClient(NodeInfo),
}

#[derive(Clone, Debug, PartialEq)]
enum NSHubOut {
    ToClient(U256, NetworkOut),
}

struct NSHub {
    nodes: Vec<NodeInfo>,
}

impl NSHub {
    async fn new() -> Result<Broker<NSHubIn, NSHubOut>, BrokerError> {
        let mut b = Broker::new();
        b.add_handler(Box::new(Self { nodes: vec![] })).await?;
        Ok(b)
    }

    fn net_msg(&self, id: U256, msg: NetworkIn) -> Vec<NSHubOut> {
        match msg {
            NetworkIn::MessageToNode(id_dst, msg_node) => {
                vec![NSHubOut::ToClient(
                    id_dst,
                    NetworkOut::MessageFromNode(id, msg_node),
                )]
            }
            NetworkIn::WSUpdateListRequest => {
                vec![NSHubOut::ToClient(
                    id,
                    NetworkOut::NodeListFromWS(self.nodes.clone()),
                )]
            }
            _ => {
                vec![]
            }
        }
    }
}

#[platform_async_trait()]
impl SubsystemHandler<NSHubIn, NSHubOut> for NSHub {
    async fn messages(&mut self, msgs: Vec<NSHubIn>) -> Vec<NSHubOut> {
        let mut out = vec![];

        for msg in msgs {
            match msg {
                NSHubIn::FromClient(id, net_msg) => {
                    out.append(&mut self.net_msg(id, net_msg));
                }
                NSHubIn::NewClient(info) => {
                    self.nodes.push(info);
                }
            }
        }

        out.into_iter().map(|msg| msg).collect()
    }
}
