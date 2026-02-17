use flarch::{
    add_translator_o_ti,
    broker::{Broker, SubsystemHandler},
    nodeids::NodeID,
    platform_async_trait,
};
use flmodules::{
    dht_router::broker::DHTRouterOut,
    dht_storage::{self, broker::DHTStorageOut},
    network::{broker::NetworkOut, signal::FledgerConfig},
    nodeconfig::NodeInfo,
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::{
    darealm::RealmID,
    proxy::{
        inter_tab::TabID,
        proxy::{Proxy, ProxyOut},
    },
};

#[derive(Debug, Clone)]
pub enum StateOut {
    NewState(State),
    Update(StateUpdate),
}

#[derive(Debug, Clone)]
#[wasm_bindgen]
pub struct NodeState {
    state: watch::Receiver<State>,
    _broker: Broker<(), StateOut>,
}

impl NodeState {
    pub async fn new(proxy: &mut Proxy) -> anyhow::Result<NodeState> {
        let (mut int, state) = Intern::new().await?;
        add_translator_o_ti!(proxy.broker, int, InternIn::Proxy);
        add_translator_o_ti!(proxy.dht_router, int, InternIn::DHTRouter);
        add_translator_o_ti!(proxy.dht_storage, int, InternIn::DHTStorage);
        add_translator_o_ti!(proxy.network, int, InternIn::Network);
        let broker = Broker::new();
        int.add_translator_o_to(broker.clone(), Box::new(|msg| Some(msg)))
            .await?;

        Ok(NodeState {
            state,
            _broker: broker,
        })
    }
}

#[wasm_bindgen]
impl NodeState {
    pub fn get_state(&self) -> State {
        self.state.borrow().clone()
    }
}

#[derive(Debug, Clone)]
enum InternIn {
    Proxy(ProxyOut),
    DHTRouter(DHTRouterOut),
    DHTStorage(DHTStorageOut),
    Network(NetworkOut),
}

#[derive(Debug)]
struct Intern {
    state: State,
    _state_watch: watch::Sender<State>,
}

impl Intern {
    async fn new() -> anyhow::Result<(Broker<InternIn, StateOut>, watch::Receiver<State>)> {
        let (state_watch, state) = watch::channel(State::default());
        Ok((
            Broker::new_with_handler(Box::new(Intern {
                state: State::default(),
                _state_watch: state_watch,
            }))
            .await?
            .0,
            state,
        ))
    }
}

#[platform_async_trait]
impl SubsystemHandler<InternIn, StateOut> for Intern {
    async fn messages(&mut self, msgs: Vec<InternIn>) -> Vec<StateOut> {
        msgs.into_iter()
            .filter_map(|i| self.state.update(i))
            .map(|m| StateOut::Update(m))
            .collect::<Vec<_>>()
    }
}

#[wasm_bindgen]
#[derive(Default, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct State {
    config: Option<FledgerConfig>,
    realm_ids: Vec<RealmID>,
    nodes_connected_dht: Vec<NodeID>,
    nodes_online: Vec<NodeInfo>,
    dht_storage_stats: dht_storage::intern::Stats,
    leader: Option<TabID>,
    is_leader: bool,
    tab_list: Vec<TabID>,
}

impl State {
    fn update(&mut self, msg: InternIn) -> Option<StateUpdate> {
        match msg {
            InternIn::Proxy(proxy_out) => match proxy_out {
                ProxyOut::Elected => Some(StateUpdate::IsLeader),
                ProxyOut::NewLeader(_) => Some(StateUpdate::NewLeader),
                ProxyOut::TabList(list) => {
                    self.tab_list = list;
                    Some(StateUpdate::TabList)
                }
                _ => None,
            },
            InternIn::DHTRouter(out) => match out {
                DHTRouterOut::NodeList(list) => Some(if list.is_empty() {
                    StateUpdate::DisconnectNodes
                } else {
                    StateUpdate::ConnectedNodes
                }),
                DHTRouterOut::SystemRealm(_) => Some(StateUpdate::SystemRealm),
                _ => None,
            },
            InternIn::DHTStorage(msg) => match msg {
                DHTStorageOut::FloValue(_) => Some(StateUpdate::ReceivedFlo),
                DHTStorageOut::FloValues(_) => Some(StateUpdate::ReceivedFlo),
                DHTStorageOut::RealmIDs(_) => Some(StateUpdate::RealmAvailable),
                DHTStorageOut::Stats(_) => Some(StateUpdate::DHTStorageStats),
                _ => None,
            },
            InternIn::Network(msg) => match msg {
                NetworkOut::NodeListFromWS(_) => Some(StateUpdate::AvailableNodes),
                NetworkOut::SystemConfig(_) => Some(StateUpdate::ConnectSignal),
                _ => None,
            },
        }
    }
}

#[wasm_bindgen]
#[derive(Clone, Debug)]
pub enum StateUpdate {
    // Connection to the signalling server established
    ConnectSignal,
    // Number of nodes connected, always >= 1
    ConnectedNodes,
    // Available nodes on the signalling server
    AvailableNodes,
    // Lost connection to last node - signal server might still be connected
    DisconnectNodes,
    // Realm available - this might happen before any connections are set up.
    // Sends the list of available realm-IDs
    RealmAvailable,
    // Got new Flo - not really sure if it is a new version, or just generally
    // a Flo arrived.
    ReceivedFlo,
    // The status of the DHT Storage changed
    DHTStorageStats,
    // This tab is the leader
    IsLeader,
    // Other new leader elected
    NewLeader,
    // Received a new list of tabs
    TabList,
    // Got system realm
    SystemRealm,
}
