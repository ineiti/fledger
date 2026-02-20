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
        broadcast::{Broadcast, BroadcastFromTabs, BroadcastToTabs, TabID},
        intern::{BrokerIntern, Intern, InternIn, InternOut},
        state::{NodeState, State, StateIn, StateOut, StateUpdate},
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
    Update(StateUpdate),
    State(State),
}

pub type BrokerProxy = Broker<ProxyIn, ProxyOut>;

#[derive(Debug, Clone)]
pub struct Proxy {
    pub broker: BrokerProxy,
    pub node_info: NodeInfo,
    pub state: NodeState,
    _intern: BrokerIntern,
}

impl Proxy {
    pub async fn start(
        ds: Box<dyn DataStorage + Send>,
        netconf: NetConf,
        tab_id: TabID,
        mut timer: BrokerTimer,
    ) -> anyhow::Result<Proxy> {
        let mut broker = Broker::new();
        let mut bc = Broadcast::start("danode", tab_id.clone()).await?;
        add_translator!(bc, o_to, broker, BroadcastFromTabs::FromLeader(msg) => ProxyOut::Update(msg));

        let node_config = node::Node::get_config(ds.clone())?;
        let mut intern =
            Intern::start(ds.clone(), node_config.clone(), tab_id.clone(), 5, netconf).await?;

        add_translator!(bc, o_ti, intern, msg => InternIn::Broadcast(msg));
        add_translator!(broker, i_ti, intern, ProxyIn::Node(m) => InternIn::Node(m));
        add_translator!(timer, o_ti, intern, TimerMessage::Second => InternIn::Timer);

        add_translator!(intern, o_ti, bc, InternOut::Broadcast(msg) => msg);
        add_translator!(intern, o_to, broker, InternOut::Node(m) => ProxyOut::Node(m));
        add_translator!(intern, o_to, broker, InternOut::Tabs(m) => ProxyOut::Tabs(m));
        add_translator!(intern, o_to, broker, InternOut::Update(m) => ProxyOut::Update(m));

        let mut state = NodeState::new(ds, tab_id).await?;
        add_translator!(broker, o_ti, state.broker, ProxyOut::Node(msg) => StateIn::Node(msg));
        add_translator!(broker, o_ti, state.broker, ProxyOut::Tabs(msg) => StateIn::Tabs(msg));
        add_translator!(bc, o_ti, state.broker, BroadcastFromTabs::FromLeader(msg) => StateIn::UpdateFromLeader(msg));

        add_translator!(state.broker, o_to, broker, StateOut::State(msg) => ProxyOut::State(msg));
        add_translator!(state.broker, o_to, broker, StateOut::Update(msg) => ProxyOut::Update(msg));
        add_translator!(state.broker, o_ti, bc, StateOut::Update(msg) => BroadcastToTabs::FromLeader(msg));

        Ok(Proxy {
            broker,
            _intern: intern,
            state,
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
