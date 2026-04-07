//! Proxies the node between different nodes. Upon start, the tabs listen
//! to other messages, and then decide on a leader as the tab which started
//! first (plus some randomness for reloading tabs).

use flarch::{
    add_translator,
    broker::Broker,
    data_storage::{DataStorage, DataStorageIndexedDB},
};
use flmodules::{
    dht_router::broker::DHTRouterIn,
    dht_storage::broker::DHTStorageIn,
    nodeconfig::NodeInfo,
    timer::{BrokerTimer, TimerMessage},
};
use flnode::node;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::{
    danode::NetConf,
    proxy::{
        broadcast::{Broadcast, TabID},
        intern::{Intern, InternIn, InternOut},
        state::{State, StateUpdate},
    },
};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ProxyIn {
    Node(NodeIn),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum NodeIn {
    DHTRouter(DHTRouterIn),
    DHTStorage(DHTStorageIn),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ProxyOut {
    Update(StateUpdate),
}

pub type BrokerProxy = Broker<ProxyIn, ProxyOut>;

#[derive(Debug, Clone)]
pub struct Proxy {
    pub broker: BrokerProxy,
    pub node_info: NodeInfo,
    pub state: watch::Receiver<State>,
}

impl Proxy {
    pub async fn start(
        ds: Box<DataStorageIndexedDB>,
        netconf: NetConf,
        tab_id: TabID,
        mut timer: BrokerTimer,
    ) -> anyhow::Result<Proxy> {
        let mut broker = Broker::new();
        let mut bc = Broadcast::start("danode", tab_id.clone()).await?;
        let node_config = node::Node::get_config(ds.clone_box())?;
        let (mut intern, state) =
            Intern::start(ds, node_config.clone(), tab_id.clone(), 2, netconf).await?;

        add_translator!(timer, o_ti, intern, TimerMessage::Second => InternIn::Timer);
        add_translator!(broker, i_ti, intern, ProxyIn::Node(m) => InternIn::NodeIn(m));
        add_translator!(bc, o_ti, intern, msg => InternIn::Broadcast(msg));

        add_translator!(intern, o_ti, bc, InternOut::Broadcast(msg) => msg);
        add_translator!(intern, o_to, broker, InternOut::Update(m) => ProxyOut::Update(m));

        Ok(Proxy {
            broker,
            state,
            node_info: node_config.info,
        })
    }
}
