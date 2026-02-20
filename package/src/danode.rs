use flarch::broker::Broker;
use flarch::data_storage::DataStorageIndexedDB;
use flarch::tasks::spawn_local;
use flmodules::dht_storage::broker::DHTStorage;
use flmodules::timer::Timer;
use js_sys::{Function, JsString};
use wasm_bindgen::prelude::*;

use crate::darealm::{DaRealm, RealmID};
use crate::proxy::broadcast::TabID;
use crate::proxy::proxy::{BrokerProxy, Proxy};
use crate::state::NodeState;
use crate::status_bar::StatusBar;

/// Main DaNode interface for browser
#[wasm_bindgen]
pub struct DaNode {
    state: NodeState,
    ds: DHTStorage,
    id: TabID,
    _proxy: BrokerProxy,
}

#[wasm_bindgen]
impl DaNode {
    /// Create a new DaNode instance
    pub async fn from_default() -> Result<DaNode, JsString> {
        Self::from_net_conf(NetConf::default())
            .await
            .map_err(|e| format!("While initialising DaNode: {e:?}").into())
    }

    pub async fn from_config(
        storage_name: String,
        signal_server: String,
        stun_server: Option<String>,
        turn_server: Option<String>,
    ) -> Result<DaNode, JsString> {
        Self::from_net_conf(NetConf {
            storage_name,
            signal_server,
            stun_server,
            turn_server,
        })
        .await
        .map_err(|e| format!("While initialising DaNode: {e:?}").into())
    }

    pub fn sync(&mut self) -> Result<(), String> {
        self.ds.sync().map_err(|e| format!("{e}"))
    }

    pub async fn get_realm(&mut self, _id: RealmID) -> Result<DaRealm, JsString> {
        Ok(DaRealm::new(
            self.ds
                .get_realm_view(_id.get_id())
                .await
                .map_err(|e| format!("While fetching realm: {e}"))?,
        ))
    }

    /// Get basic statistics
    pub fn get_stats(&self) -> JsValue {
        // TODO: Implement actual stats retrieval
        let stats = serde_json::json!({
            "messages_sent": 0,
            "messages_received": 0,
            "uptime_seconds": 0
        });

        serde_wasm_bindgen::to_value(&stats).unwrap_or(JsValue::NULL)
    }

    /// Set an event listener callback
    /// The callback will be called with events in the format: { type: string, data: any }
    pub async fn set_event_listener(&mut self, callback: Function) -> Result<usize, String> {
        let (mut tap, id) = self
            .state
            .broker
            .get_tap_out()
            .await
            .map_err(|e| format!("Getting tap: {e:?}"))?;

        let state = self.state.state.clone();
        spawn_local(async move {
            while let Some(msg) = tap.recv().await {
                if let Err(e) = callback.call2(
                    &JsValue::null(),
                    &msg.into(),
                    &state.borrow().clone().into(),
                ) {
                    log::error!("Couldn't call event callback: {e:?}");
                }
            }
        });

        Ok(id)
    }

    /// Remove the event listener callback
    pub async fn remove_event_listener(&mut self, id: usize) -> Result<(), String> {
        self.state
            .broker
            .remove_subsystem(id)
            .await
            .map_err(|e| format!("Couldn't remove event listener: {e:?}"))
    }

    /// Set the div element to display the status bar
    pub async fn set_status_div(&mut self, div_id: String) -> Result<usize, String> {
        let b = StatusBar::new(self.id.clone(), &div_id, self.state.state.borrow().clone())
            .await
            .map_err(|e| format!("Couldn't create new status bar: {e:?}"))?;
        self.state
            .broker
            .add_translator_o_ti(b.clone(), Box::new(|msg| Some(msg)))
            .await
            .map_err(|e| format!("While adding translator: {e:?}"))
    }

    /// Remove the status bar display
    pub async fn remove_status_div(&mut self, id: usize) -> Result<(), String> {
        self.state
            .broker
            .remove_subsystem(id)
            .await
            .map_err(|e| format!("Couldn't remove status_div from state.broker: {e:?}"))
    }

    pub fn get_tab_id(&self) -> String {
        format!("{}", self.id)
    }
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

impl DaNode {
    async fn from_net_conf(nc: NetConf) -> anyhow::Result<DaNode> {
        let id = TabID::new();

        Ok(DaNode {
            ds: DHTStorage::from_broker(Broker::new(), 1000).await?,
            _proxy: Proxy::start(
                DataStorageIndexedDB::new("node_state").await?,
                nc,
                id.clone(),
                Timer::start().await?.broker,
            )
            .await?
            .broker,
            state: NodeState::new(DataStorageIndexedDB::new("node_state").await?, id.clone())
                .await?,
            id,
        })
    }
}
