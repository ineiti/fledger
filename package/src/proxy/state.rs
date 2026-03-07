//! A common state for all tabs, so that all Flos are only stored
//! once.
//! The state is sent between the tabs using DataStorage, which
//! has the disadvantage that somtimes it's not synchronized between
//! the leader and the followers.

use std::collections::HashMap;

use flarch::{data_storage::DataStorage, nodeids::NodeID};
use flmodules::{
    dht_router::broker::DHTRouterOut,
    dht_storage::{self, broker::DHTStorageOut, core::FloCuckoo},
    flo::{
        flo::{Flo, FloID},
        realm::{self, GlobalID},
    },
    network::{broker::NetworkOut, signal::FledgerConfig},
    nodeconfig::{self},
};
use flnode::node::Node;
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::{
    ids,
    proxy::{broadcast::TabID, intern::NodeOut},
};

#[derive(Tsify)]
pub struct NodeConfig {
    pub name: String,
    pub id: NodeID,
}

#[derive(Tsify, Clone, Debug, Serialize, Deserialize, PartialEq)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct State {
    pub config: Option<FledgerConfig>,
    pub node_info: nodeconfig::NodeInfo,
    pub realm_ids: Vec<realm::RealmID>,
    pub nodes_connected_dht: Vec<NodeID>,
    pub nodes_online: Vec<nodeconfig::NodeInfo>,
    pub dht_storage_stats: dht_storage::intern::Stats,
    pub flos: HashMap<realm::RealmID, HashMap<FloID, FloCuckoo>>,
    pub is_leader: Option<bool>,
    pub tab_list: Vec<TabID>,
}

const STORAGE_STATE: &str = "danodeState";

impl State {
    pub fn read(
        ds: &mut Box<dyn DataStorage + Send>,
        leader: Option<bool>,
    ) -> anyhow::Result<Self> {
        let state_str = ds.get(STORAGE_STATE)?;
        let mut state = serde_json::from_str::<State>(&state_str)
            .ok()
            .unwrap_or_else(|| {
                let node_config = Node::get_config(ds.clone()).unwrap();
                Self {
                    config: None,
                    node_info: node_config.info,
                    realm_ids: Vec::new(),
                    nodes_connected_dht: Vec::new(),
                    nodes_online: Vec::new(),
                    dht_storage_stats: dht_storage::intern::Stats::default(),
                    flos: HashMap::new(),
                    tab_list: Vec::new(),
                    is_leader: None,
                }
            });
        state.is_leader = leader;
        Ok(state)
    }

    pub fn store(&mut self, ds: &mut Box<dyn DataStorage + Send>) {
        if let Ok(s) = serde_json::to_string(self) {
            if let Err(e) = ds.set(STORAGE_STATE, &s) {
                log::warn!("Couldn't store: {e:?}");
            }
        }
    }

    pub fn msg_new_tabs(&mut self, tab_list: Vec<TabID>) -> StateUpdate {
        self.tab_list = tab_list;
        StateUpdate::TabList
    }

    pub fn msg_node(&mut self, msg: NodeOut) -> Vec<StateUpdate> {
        vec![match msg {
            NodeOut::DHTRouter(out) => match out {
                DHTRouterOut::NodeList(list) => {
                    if self.nodes_connected_dht != list {
                        self.nodes_connected_dht = list;
                        if self.nodes_connected_dht.is_empty() {
                            StateUpdate::DisconnectNodes
                        } else {
                            StateUpdate::ConnectedNodes
                        }
                    } else {
                        return vec![];
                    }
                }
                DHTRouterOut::SystemRealm(_) => StateUpdate::SystemRealm,
                _ => return vec![],
            },
            NodeOut::DHTStorage(msg) => match msg {
                DHTStorageOut::FloValue(fv) => {
                    self.flos
                        .entry(fv.0.realm_id())
                        .or_insert_with(HashMap::new)
                        .insert(fv.0.flo_id(), fv.clone());
                    StateUpdate::ReceivedFlo(fv.0.flo_id())
                }
                DHTStorageOut::FloValues(fvs) => {
                    let mut ret = vec![];
                    for fv in fvs {
                        ret.push(StateUpdate::ReceivedFlo(fv.0.flo_id()));
                        self.flos
                            .entry(fv.0.realm_id())
                            .or_insert_with(HashMap::new)
                            .insert(fv.0.flo_id(), fv);
                    }
                    return ret;
                }
                DHTStorageOut::RealmIDs(rids) => {
                    self.realm_ids = rids;
                    StateUpdate::RealmAvailable
                }
                DHTStorageOut::Stats(st) => {
                    self.dht_storage_stats = st;
                    StateUpdate::DHTStorageStats
                }
                _ => return vec![],
            },
            NodeOut::Network(msg) => match msg {
                NetworkOut::NodeListFromWS(list) => {
                    if self.nodes_online != list {
                        self.nodes_online = list;
                        StateUpdate::AvailableNodes
                    } else {
                        return vec![];
                    }
                }
                NetworkOut::SystemConfig(sc) => {
                    self.config = Some(sc);
                    StateUpdate::ConnectSignal
                }
                _ => return vec![],
            },
        }]
    }

    pub fn get_flo(&self, gid: &GlobalID) -> Option<Flo> {
        self.get_flo_cuckoos(gid).map(|(flo, _)| flo)
    }

    pub fn get_flo_cuckoos(&self, gid: &GlobalID) -> Option<FloCuckoo> {
        self.flos
            .get(gid.realm_id())
            .and_then(|realm| realm.get(gid.flo_id()))
            .cloned()
    }
}

#[wasm_bindgen]
impl State {
    pub fn get_system_realm(&self) -> Option<ids::RealmID> {
        self.config
            .as_ref()
            .and_then(|conf| conf.system_realm.as_ref())
            .map(|rid| ids::RealmID::new(rid.clone()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    ReceivedFlo(FloID),
    // The status of the DHT Storage changed
    DHTStorageStats,
    // New leader elected
    NewLeader,
    // Received a new list of tabs
    TabList,
    // Got system realm
    SystemRealm,
}
