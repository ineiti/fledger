use flmodules::dht_storage::broker::{DHTStorage, DHTStorageIn};
use flmodules::timer::Timer;
use js_sys::{Function, JsString};
use wasm_bindgen::prelude::*;

use crate::darealm::{DaRealm, RealmID};
use crate::events::{BrokerEvents, Events, EventsIn};
use crate::proxy::broadcast::Broadcast;
use crate::proxy::intern::TabID;
use crate::proxy::proxy::Proxy;

/// Main DaNode interface for browser
#[wasm_bindgen]
pub struct DaNode {
    events: BrokerEvents,
    _proxy: Proxy,
    ds: DHTStorage,
    id: TabID,
}

#[wasm_bindgen]
impl DaNode {
    /// Create a new DaNode instance
    pub async fn from_default() -> Result<DaNode, JsString> {
        Self::from_net_conf(NetConf::default()).await
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
    pub fn set_event_listener(&mut self, callback: Function) -> Result<(), String> {
        self.events
            .emit_msg_in(EventsIn::EventHandler(callback))
            .map_err(|e| format!("{e}"))?;
        Ok(())
    }

    /// Remove the event listener callback
    pub fn remove_event_listener(&mut self) -> Result<(), String> {
        self.events
            .emit_msg_in(EventsIn::ClearEvent)
            .map_err(|e| format!("{e}"))?;
        Ok(())
    }

    pub async fn update_realms(&mut self) -> Result<(), String> {
        self.ds
            .broker
            .emit_msg_in(DHTStorageIn::GetRealms)
            .map_err(|e| format!("{e:?}"))
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
    async fn from_net_conf(nc: NetConf) -> Result<DaNode, JsString> {
        let id = TabID::new();
        let mut _proxy = Proxy::start(
            id.clone(),
            nc,
            &mut Timer::start().await.map_err(|e| format!("{e}"))?.broker,
            Broadcast::start("danode", id.clone())
                .await
                .map_err(|e| format!("{e}"))?,
        )
        .await
        .map_err(|e| format!("{e}"))?;

        Ok(DaNode {
            events: Events::new(&mut _proxy).await.map_err(|e| format!("{e}"))?,
            ds: DHTStorage::from_broker(_proxy.dht_storage.clone(), 1000)
                .await
                .map_err(|e| format!("{e}"))?,
            id,
            _proxy,
        })
    }
}
