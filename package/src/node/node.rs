use flarch::broker::Broker;
use flmodules::{
    dht_router::broker::{DHTRouterIn, DHTRouterOut},
    dht_storage::broker::{DHTStorageIn, DHTStorageOut},
    network::broker::NetworkOut,
    nodeconfig::NodeInfo,
};
use serde::{Deserialize, Serialize};

use crate::{danode::NetConf, proxy::inter_tab::TabID};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum NodeIn {
    DHTRouter(DHTRouterIn),
    DHTStorage(DHTStorageIn),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum NodeOut {
    DHTRouter(DHTRouterOut),
    DHTStorage(DHTStorageOut),
    // network only sends NetworkOut::SystemConfig and NetworkOut::NodeListFromWS.
    Network(NetworkOut),
}

pub type BrokerNode = Broker<NodeIn, NodeOut>;

#[derive(Debug, Clone)]
pub struct Node {
    pub broker: BrokerNode,
    pub tab_id: TabID,
    pub netconf: NetConf,
    pub node_info: NodeInfo,
}

impl Node {
    pub fn start(_nc: NetConf) -> anyhow::Result<BrokerNode> {
        // let id = TabID::new();
        // let mut _proxy = Proxy::start(
        //     id.clone(),
        //     nc,
        //     &mut Timer::start().await.map_err(|e| format!("{e}"))?.broker,
        //     Broadcast::start("danode", id.clone())
        //         .await
        //         .map_err(|e| format!("{e}"))?,
        // )
        // .await
        // .map_err(|e| format!("{e}"))?;
        todo!()
    }
}
