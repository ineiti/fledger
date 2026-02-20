use flarch::{add_translator, broker::Broker, data_storage::DataStorage};
use flmodules::{
    dht_router::broker::{DHTRouterIn, DHTRouterOut},
    dht_storage::broker::{DHTStorageIn, DHTStorageOut},
    network::broker::NetworkOut,
    nodeconfig::NodeInfo,
    timer::{BrokerTimer, TimerMessage},
};
use flnode::node;
use serde::{Deserialize, Serialize};

use crate::{
    danode::NetConf,
    proxy::{
        broadcast::{Broadcast, TabID},
        intern::{BrokerIntern, Intern, InternIn, InternOut},
    },
};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ProxyIn {
    Node(NodeIn),
    IsLeader,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ProxyOut {
    Node(NodeOut),
    Tabs(Tabs),
}

pub type BrokerProxy = Broker<ProxyIn, ProxyOut>;

#[derive(Debug, Clone)]
pub struct Proxy {
    pub broker: BrokerProxy,
    pub node_info: NodeInfo,
    _intern: BrokerIntern,
}

impl Proxy {
    pub async fn start(
        ds: Box<dyn DataStorage + Send>,
        netconf: NetConf,
        tab_id: TabID,
        mut timer: BrokerTimer,
    ) -> anyhow::Result<Proxy> {
        let mut bc = Broadcast::start("danode", tab_id.clone()).await?;
        let mut broker = Broker::new();
        let node_config = node::Node::get_config(ds.clone())?;
        let mut intern = Intern::start(ds.clone(), node_config.clone(), tab_id, 5, netconf).await?;

        add_translator!(bc, o_ti, intern, msg => InternIn::Broadcast(msg));
        add_translator!(intern, o_ti, bc, InternOut::Broadcast(msg) => msg);
        add_translator!(intern, o_to, broker, InternOut::Node(m) => ProxyOut::Node(m));
        add_translator!(broker, i_ti, intern, ProxyIn::Node(m) => InternIn::Node(m));
        add_translator!(timer, o_ti, intern, TimerMessage::Second => InternIn::Timer);

        Ok(Proxy {
            broker,
            _intern: intern,
            node_info: node_config.info,
        })
    }
}

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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum Tabs {
    Elected,
    NewLeader(TabID),
    TabList(Vec<TabID>),
}
