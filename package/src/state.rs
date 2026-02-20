use std::collections::HashMap;

use flarch::{
    broker::{Broker, SubsystemHandler},
    data_storage::DataStorage,
    nodeids::NodeID,
    platform_async_trait,
};
use flmodules::{
    dht_router::broker::DHTRouterOut,
    dht_storage::{self, broker::DHTStorageOut, core::FloCuckoo},
    flo::{
        flo::FloID,
        realm::{self, RealmID},
    },
    network::{broker::NetworkOut, signal::FledgerConfig},
    nodeconfig::{self},
};
use flnode::node::Node;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tsify::Tsify;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::proxy::{broadcast::TabID, proxy::NodeOut};

#[derive(Debug, Clone)]
pub enum StateIn {
    Node(NodeOut),
}

#[derive(Tsify, Debug, Clone, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum StateOut {
    State(State),
    Update(StateUpdate),
}

#[derive(Debug, Clone)]
pub struct NodeState {
    pub state: watch::Receiver<State>,
    pub broker: Broker<StateIn, StateOut>,
}

impl NodeState {
    pub async fn new(ds: Box<dyn DataStorage + Send>) -> anyhow::Result<NodeState> {
        let (broker, state) = Intern::new(ds).await?;

        Ok(NodeState { state, broker })
    }
}

#[derive(Debug)]
struct Intern {
    state: State,
    _state_update: watch::Sender<State>,
}

impl Intern {
    async fn new(
        ds: Box<dyn DataStorage + Send>,
    ) -> anyhow::Result<(Broker<StateIn, StateOut>, watch::Receiver<State>)> {
        let state = State::new(ds)?;
        let (_state_update, state_watch) = watch::channel(state.clone());
        Ok((
            Broker::new_with_handler(Box::new(Intern {
                state,
                _state_update,
            }))
            .await?
            .0,
            state_watch,
        ))
    }
}

#[platform_async_trait]
impl SubsystemHandler<StateIn, StateOut> for Intern {
    async fn messages(&mut self, msgs: Vec<StateIn>) -> Vec<StateOut> {
        let mut out = msgs
            .into_iter()
            .filter_map(|StateIn::Node(i)| self.state.update(i))
            .map(|m| StateOut::Update(m))
            .collect::<Vec<_>>();
        if !out.is_empty() {
            out.insert(0, StateOut::State(self.state.clone()));
        }
        out
    }
}

#[derive(Tsify)]
pub struct NodeConfig {
    pub name: String,
    pub id: NodeID,
}

#[derive(Tsify, Clone, Debug, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct State {
    pub config: Option<FledgerConfig>,
    pub node_info: nodeconfig::NodeInfo,
    pub realm_ids: Vec<RealmID>,
    pub nodes_connected_dht: Vec<NodeID>,
    pub nodes_online: Vec<nodeconfig::NodeInfo>,
    pub dht_storage_stats: dht_storage::intern::Stats,
    pub leader: Option<TabID>,
    pub is_leader: bool,
    pub tab_list: Vec<TabID>,
    pub flos: HashMap<realm::RealmID, HashMap<FloID, FloCuckoo>>,
    #[serde(skip)]
    ds: Option<Box<dyn DataStorage + Send>>,
}

const STORAGE_CONFIG: &str = "danodeState";

impl State {
    fn new(ds: Box<dyn DataStorage + Send>) -> anyhow::Result<Self> {
        if let Ok(s) = ds.get(STORAGE_CONFIG) {
            if let Ok(state) = serde_json::from_str::<State>(&s) {
                return Ok(state);
            }
        }
        let node_config = Node::get_config(ds.clone())?;
        let mut state = Self {
            config: None,
            node_info: node_config.info,
            realm_ids: Vec::new(),
            nodes_connected_dht: Vec::new(),
            nodes_online: Vec::new(),
            dht_storage_stats: dht_storage::intern::Stats::default(),
            leader: None,
            is_leader: false,
            tab_list: Vec::new(),
            flos: HashMap::new(),
            ds: Some(ds),
        };

        state.store();
        Ok(state)
    }

    pub fn get_system_realm(&self) -> Option<RealmID> {
        self.config
            .as_ref()
            .and_then(|conf| conf.system_realm.clone())
    }

    fn store(&mut self) {
        if let Ok(s) = serde_json::to_string(self) {
            if let Some(ds) = &mut self.ds {
                if let Err(e) = ds.set("DANODE_STATE", &s) {
                    log::warn!("Couldn't store: {e:?}");
                }
            }
        }
    }

    fn update(&mut self, msg: NodeOut) -> Option<StateUpdate> {
        let out = match msg {
            // NodeOut::IsLeader => return None,
            // NodeOut::Proxy(proxy_out) => match proxy_out {
            //     ProxyOut::Elected => {
            //         self.is_leader = true;
            //         StateUpdate::IsLeader
            //     }
            //     ProxyOut::NewLeader(leader) => {
            //         self.leader = Some(leader);
            //         StateUpdate::NewLeader
            //     }
            //     ProxyOut::TabList(list) => {
            //         self.tab_list = list;
            //         StateUpdate::TabList
            //     }
            //     _ => return None,
            // },
            NodeOut::DHTRouter(out) => match out {
                DHTRouterOut::NodeList(list) => {
                    self.nodes_connected_dht = list;
                    if self.nodes_connected_dht.is_empty() {
                        StateUpdate::DisconnectNodes
                    } else {
                        StateUpdate::ConnectedNodes
                    }
                }
                DHTRouterOut::SystemRealm(_) => StateUpdate::SystemRealm,
                _ => return None,
            },
            NodeOut::DHTStorage(msg) => match msg {
                DHTStorageOut::FloValue(fv) => {
                    self.flos
                        .entry(fv.0.realm_id())
                        .or_insert_with(HashMap::new)
                        .insert(fv.0.flo_id(), fv);
                    StateUpdate::ReceivedFlo
                }
                DHTStorageOut::FloValues(fvs) => {
                    for fv in fvs {
                        self.flos
                            .entry(fv.0.realm_id())
                            .or_insert_with(HashMap::new)
                            .insert(fv.0.flo_id(), fv);
                    }
                    StateUpdate::ReceivedFlo
                }
                DHTStorageOut::RealmIDs(rids) => {
                    self.realm_ids = rids;
                    StateUpdate::RealmAvailable
                }
                DHTStorageOut::Stats(st) => {
                    self.dht_storage_stats = st;
                    StateUpdate::DHTStorageStats
                }
                _ => return None,
            },
            NodeOut::Network(msg) => match msg {
                NetworkOut::NodeListFromWS(list) => {
                    self.nodes_online = list;
                    StateUpdate::AvailableNodes
                }
                NetworkOut::SystemConfig(sc) => {
                    self.config = Some(sc);
                    StateUpdate::ConnectSignal
                }
                _ => return None,
            },
        };
        self.store();
        Some(out)
    }
}

#[derive(Tsify, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[tsify(into_wasm_abi, from_wasm_abi)]
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
