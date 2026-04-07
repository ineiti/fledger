//! DaNode is the basic class for the typescript library.

use flarch::add_translator;
use flarch::data_storage::DataStorageIndexedDB;
use flmodules::dht_storage::broker::DHTStorageIn;
use flmodules::timer::Timer;
use serde::Serialize;
use wasm_bindgen::prelude::*;
use web_sys::ReadableStream;

use crate::darealm::RealmObserver;
use crate::error::{WasmError, WasmResult};
use crate::ids::RealmID;
use crate::proxy::broadcast::TabID;
use crate::proxy::proxy::{NodeIn, Proxy, ProxyIn, ProxyOut};
use crate::proxy::state::{State, StateUpdate};
use crate::status_bar::{StatusBar, StatusBarIn};

/// Main DaNode interface for browser
#[wasm_bindgen]
pub struct DaNode {
    id: TabID,
    proxy: Proxy,
}

#[wasm_bindgen]
impl DaNode {
    /// Create a new DaNode instance
    pub async fn from_default() -> Result<DaNode, WasmError> {
        Ok(Self::from_net_conf(NetConf::default()).await?)
    }

    pub async fn from_config(
        storage_name: String,
        signal_server: String,
        stun_server: Option<String>,
        turn_server: Option<String>,
    ) -> Result<DaNode, WasmError> {
        Ok(Self::from_net_conf(NetConf {
            storage_name,
            signal_server,
            stun_server,
            turn_server,
        })
        .await?)
    }

    pub fn sync(&mut self) -> Result<(), WasmError> {
        Ok(self
            .proxy
            .broker
            .emit_msg_in(ProxyIn::Node(NodeIn::DHTStorage(
                DHTStorageIn::SyncFromNeighbors,
            )))?)
    }

    pub async fn get_state(&mut self) -> WasmResult<StateInit> {
        let snapshot = self.proxy.state.clone();
        let initial = snapshot.borrow().clone();
        let (stream, _) = self.proxy.broker
            .get_async_iterable_out_translate(Box::new(move |msg| {
                let ProxyOut::Update(update) = msg;
                Some(StateUpdateMsg {
                    state: snapshot.borrow().clone(),
                    update,
                })
            }))
            .await?;
        Ok(StateInit { stream, state: initial })
    }

    pub async fn get_realm(&mut self, id: RealmID) -> Result<RealmObserver, WasmError> {
        Ok(RealmObserver::start(
            self.proxy.broker.clone(),
            self.proxy.state.clone(),
            id.into(),
        )
        .await?)
    }

    /// Set the div element to display the status bar
    pub async fn set_status_div(&mut self, div_id: String) -> Result<usize, WasmError> {
        Ok(self.ssd(div_id).await?)
    }

    /// Remove the status bar display
    pub async fn remove_status_div(&mut self, id: usize) -> Result<(), WasmError> {
        Ok(self.proxy.broker.remove_subsystem(id).await?)
    }

    pub fn get_tab_id(&self) -> String {
        format!("{}", self.id)
    }
}

impl DaNode {
    async fn from_net_conf(nc: NetConf) -> anyhow::Result<DaNode> {
        let id = TabID::new();

        Ok(DaNode {
            proxy: Proxy::start(
                DataStorageIndexedDB::new("node_state").await?,
                nc,
                id.clone(),
                Timer::start().await?.broker,
            )
            .await?,
            id,
        })
    }

    async fn ssd(&mut self, div_id: String) -> anyhow::Result<usize> {
        let b = StatusBar::new(self.id.clone(), &div_id, self.proxy.state.clone()).await?;
        Ok(
            add_translator!(self.proxy.broker, o_ti, b, ProxyOut::Update(su) => StatusBarIn::Update(su)),
        )
    }
}

#[derive(Debug, Serialize, Clone)]
struct StateUpdateMsg {
    update: StateUpdate,
    state: State,
}

#[wasm_bindgen]
pub struct StateInit {
    #[wasm_bindgen(readonly, getter_with_clone)]
    pub stream: ReadableStream,
    #[wasm_bindgen(readonly, getter_with_clone)]
    pub state: State,
}

#[derive(Debug, Clone)]
pub struct NetConf {
    pub storage_name: String,
    pub signal_server: String,
    pub stun_server: Option<String>,
    pub turn_server: Option<String>,
}

impl Default for NetConf {
    fn default() -> Self {
        #[cfg(not(feature = "local"))]
        return Self {
            storage_name: "danu".into(),
            signal_server: "wss://signal.fledg.re".into(),
            stun_server: Some("stun:stun.l.google.com:19302".into()),
            turn_server: Some("something:something@turn:web.fledg.re:3478".into()),
        };

        #[cfg(feature = "local")]
        return Self {
            storage_name: "danu_local".into(),
            signal_server: "ws://localhost:8765".into(),
            stun_server: None,
            turn_server: None,
        };
    }
}
