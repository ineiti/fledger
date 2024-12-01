use flarch::{
    broker_io::{BrokerError, BrokerIO, SubsystemHandler, Translate},
    nodeids::{NodeID, U256},
    platform_async_trait,
};

use crate::nodeconfig::{NodeConfig, NodeInfo};

use super::messages::{OverlayIn, OverlayOut};

pub struct NetworkBrokerSimul {
    nsh_broker: BrokerIO<NSHubMessageIn, NSHubMessageOut>,
}

impl NetworkBrokerSimul {
    pub async fn new() -> Result<Self, BrokerError> {
        let mut nsh_broker = BrokerIO::new();
        nsh_broker
            .add_handler(Box::new(NSHub { nodes: vec![] }))
            .await?;
        Ok(Self { nsh_broker })
    }

    pub async fn new_node(
        &mut self,
    ) -> Result<(NodeConfig, BrokerIO<OverlayIn, OverlayOut>), BrokerError> {
        self.new_node_id(None).await
    }

    pub async fn new_node_id(
        &mut self,
        id_opt: Option<NodeID>,
    ) -> Result<(NodeConfig, BrokerIO<OverlayIn, OverlayOut>), BrokerError> {
        let (id, nc) = if let Some(id) = id_opt {
            (id, NodeConfig::new_id(id))
        } else {
            let nc = NodeConfig::new();
            (nc.info.get_id(), nc)
        };
        let nm_broker = BrokerIO::new();

        self.nsh_broker
            .add_translator_direct(
                nm_broker.clone(),
                Self::net_nsh(id),
                Self::nsh_net(id.clone()),
            )
            .await?;
        self.nsh_broker
            .emit_msg_in(NSHubMessageIn::NewClient(nc.info.clone()))?;

        Ok((nc, nm_broker))
    }

    fn nsh_net(our_id: U256) -> Translate<NSHubMessageOut, OverlayOut> {
        Box::new(move |msg| {
            let NSHubMessageOut::ToClient(dst, net_msg) = msg;
            return (dst == our_id).then_some(net_msg);
        })
    }

    fn net_nsh(our_id: U256) -> Translate<OverlayIn, NSHubMessageIn> {
        Box::new(move |msg| Some(NSHubMessageIn::FromClient(our_id, msg)))
    }
}

#[derive(Clone, Debug, PartialEq)]
enum NSHubMessageIn {
    FromClient(U256, OverlayIn),
    NewClient(NodeInfo),
}

#[derive(Clone, Debug, PartialEq)]
enum NSHubMessageOut {
    ToClient(U256, OverlayOut),
}

struct NSHub {
    nodes: Vec<NodeInfo>,
}

impl NSHub {
    fn net_msg(&self, id: U256, msg: OverlayIn) -> Vec<NSHubMessageOut> {
        let OverlayIn::NetworkWrapperToNetwork(id_dst, msg_node) = msg;
        log::debug!("{id} -> {id_dst}: {:?}", msg_node);
        vec![NSHubMessageOut::ToClient(
            id_dst,
            OverlayOut::NetworkWrapperFromNetwork(id, msg_node),
        )]
    }
}

#[platform_async_trait()]
impl SubsystemHandler<NSHubMessageIn, NSHubMessageOut> for NSHub {
    async fn messages(&mut self, msgs: Vec<NSHubMessageIn>) -> Vec<NSHubMessageOut> {
        let mut out = vec![];

        for msg in msgs {
            match msg {
                NSHubMessageIn::FromClient(id, net_msg) => {
                    out.append(&mut self.net_msg(id, net_msg));
                }
                NSHubMessageIn::NewClient(info) => {
                    self.nodes.push(info);
                }
            }
        }

        out
    }
}
