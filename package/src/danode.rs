//! DaNode is the basic class for the typescript library.

use flarch::add_translator;
use flarch::broker::Broker;
use flarch::data_storage::DataStorageIndexedDB;
use flarch::tasks::spawn_local;
use flmodules::dht_storage::broker::DHTStorage;
use flmodules::timer::Timer;
use js_sys::{Function, JsString};
use wasm_bindgen::prelude::*;

use crate::darealm::{DaRealm, RealmID};
use crate::proxy::broadcast::TabID;
use crate::proxy::proxy::{Proxy, ProxyOut};
use crate::status_bar::{StatusBar, StatusBarIn};

/// Main DaNode interface for browser
#[wasm_bindgen]
pub struct DaNode {
    ds: DHTStorage,
    id: TabID,
    proxy: Proxy,
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

    /// Set an event listener callback
    /// The callback will be called with events in the format: { type: string, data: any }
    pub async fn set_event_listener(&mut self, callback: Function) -> Result<usize, String> {
        let (mut tap, id) = self
            .proxy
            .broker
            .get_tap_out()
            .await
            .map_err(|e| format!("Getting tap: {e:?}"))?;

        let state = self.proxy.state.clone();
        spawn_local(async move {
            while let Some(ProxyOut::Update(msg)) = tap.recv().await {
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
        self.proxy
            .broker
            .remove_subsystem(id)
            .await
            .map_err(|e| format!("Couldn't remove event listener: {e:?}"))
    }

    /// Set the div element to display the status bar
    pub async fn set_status_div(&mut self, div_id: String) -> Result<usize, String> {
        self.ssd(div_id)
            .await
            .map_err(|e| format!("While setting status_div - {e:?}"))
    }

    /// Remove the status bar display
    pub async fn remove_status_div(&mut self, id: usize) -> Result<(), String> {
        self.proxy
            .broker
            .remove_subsystem(id)
            .await
            .map_err(|e| format!("Couldn't remove status_div from state.broker: {e:?}"))
    }

    pub fn get_tab_id(&self) -> String {
        format!("{}", self.id)
    }
}

impl DaNode {
    async fn from_net_conf(nc: NetConf) -> anyhow::Result<DaNode> {
        let id = TabID::new();

        Ok(DaNode {
            ds: DHTStorage::from_broker(Broker::new(), 1000).await?,
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
