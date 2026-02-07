use flarch::data_storage::DataStorageLocal;
use flarch::web_rtc::connection::{ConnectionConfig, HostLogin};
use flmodules::Modules;
use flnode::node::Node;
use js_sys::{Function, JsString};
use log::info;
use wasm_bindgen::prelude::*;

use crate::darealm::{DaRealm, RealmID};
use crate::events::{BrokerEvents, Events, EventsIn};

#[derive(Debug)]
#[wasm_bindgen]
pub struct NetConf {
    storage_name: &'static str,
    signal_server: &'static str,
    stun_server: Option<&'static str>,
    turn_server: Option<&'static str>,
}

#[cfg(not(feature = "local"))]
const NETWORK_CONFIG: NetConf = NetConf {
    storage_name: "danu",
    signal_server: "wss://signal.fledg.re",
    stun_server: Some("stun:stun.l.google.com:19302"),
    turn_server: Some("something:something@turn:web.fledg.re:3478"),
};

#[cfg(feature = "local")]
const NETWORK_CONFIG: NetConf = NetConf {
    storage_name: "danu_local",
    signal_server: "ws://localhost:8765",
    stun_server: None,
    turn_server: None,
};

/// Main DaNode interface for browser
#[wasm_bindgen]
pub struct DaNode {
    node: Node,
    events: BrokerEvents,
}

#[wasm_bindgen]
impl DaNode {
    /// Create a new DaNode instance
    pub async fn from_default() -> Result<DaNode, JsString> {
        info!("Creating new DaNode instance");
        Self::from_config(NETWORK_CONFIG).await
    }

    pub async fn from_config(cfg: NetConf) -> Result<DaNode, JsString> {
        let my_storage = DataStorageLocal::new(cfg.storage_name);
        let mut node_config = Node::get_config(my_storage.clone())
            .map_err(|e| format!("Failed to get config: {}", e))?;
        let config = ConnectionConfig::new(
            cfg.signal_server.into(),
            cfg.stun_server
                .and_then(|url| Some(HostLogin::from_url(url))),
            cfg.turn_server
                .and_then(|url| HostLogin::from_login_url(url).ok()),
        );
        node_config.info.modules = Modules::stable() - Modules::WEBPROXY_REQUESTS;
        let mut node = Node::start_network(my_storage, node_config, config)
            .await
            .map_err(|e| format!("Failed to start network: {}", e))?;
        Ok(DaNode {
            events: Events::new(&mut node).await.map_err(|e| format!("{e}"))?,
            node,
        })
    }

    pub async fn get_realm(&mut self, id: RealmID) -> Result<DaRealm, JsString> {
        let ds = self
            .node
            .dht_storage
            .as_mut()
            .ok_or(format!("DHTStorage not yet initialized"))?;
        let realm = ds
            .get_realm_view(id.get_id())
            .await
            .map_err(|e| format!("While fetching realm: {e}"))?;
        Ok(DaRealm::new(realm))
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
            .emit_msg_in(EventsIn::Event(callback))
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
}
