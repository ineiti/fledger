use std::sync::Mutex;

use flarch::{
    broker::{Broker, SubsystemHandler},
    platform_async_trait,
};
use flmodules::{
    dht_router::broker::DHTRouterOut, dht_storage::broker::DHTStorageOut,
    network::broker::NetworkOut, nodeconfig::NodeInfo,
};

use js_sys::Function;
use wasm_bindgen::prelude::*;

use crate::{
    darealm::{FloID, RealmID},
    proxy::proxy::{Proxy, ProxyOut},
    status_bar::StatusBar,
};

#[derive(Debug, Clone)]
pub enum EventsIn {
    DhtStorage(DHTStorageOut),
    DhtRouter(DHTRouterOut),
    Network(NetworkOut),
    Proxy(ProxyOut),
    NodeInfo(NodeInfo),
    EventHandler(Function),
    ClearEvent,
    SetStatusDiv(String),
    ClearStatusDiv,
}

pub type BrokerEvents = Broker<EventsIn, ()>;

#[derive(Clone)]
struct StorageStats {
    total_size: usize,
    realm_count: usize,
    page_count: usize,
}

pub struct Events {
    event_callback: Mutex<Option<Function>>,
    connected: bool,
    realms: Vec<RealmID>,
    nodes: usize,
    is_leader: bool,
    status_bar: Option<StatusBar>,
    node_info: Option<NodeInfo>,
    connected_node_ids: Vec<String>,
    storage_stats: Option<StorageStats>,
    total_pages: usize,
    available_nodes: usize,
}

impl Events {
    pub async fn new(proxy: &mut Proxy) -> Result<BrokerEvents, anyhow::Error> {
        let b = Broker::new_with_handler(Box::new(Events {
            event_callback: Mutex::new(None),
            connected: false,
            realms: vec![],
            nodes: 0,
            is_leader: false,
            status_bar: None,
            node_info: None,
            connected_node_ids: vec![],
            storage_stats: None,
            total_pages: 0,
            available_nodes: 0,
        }))
        .await?
        .0;
        proxy
            .broker
            .add_translator_o_ti(b.clone(), Box::new(|o| Some(EventsIn::Proxy(o))))
            .await?;
        proxy
            .network
            .add_translator_o_ti(b.clone(), Box::new(|o| Some(EventsIn::Network(o))))
            .await?;
        proxy
            .dht_router
            .add_translator_o_ti(b.clone(), Box::new(|o| Some(EventsIn::DhtRouter(o))))
            .await?;
        proxy
            .dht_storage
            .add_translator_o_ti(b.clone(), Box::new(|o| Some(EventsIn::DhtStorage(o))))
            .await?;
        return Ok(b);
    }

    fn emit_event(&self, event_type: NodeStatus, data: JsValue) {
        if let Some(callback) = self.event_callback.lock().unwrap().as_ref() {
            // Call the callback with the event object
            if let Err(e) = callback.call2(&JsValue::NULL, &event_type.into(), &data) {
                log::error!("Error calling event callback: {:?}", e);
            }
        }
    }

    fn update_status_bar(&self) {
        if let Some(bar) = &self.status_bar {
            // Update nodes count
            let nodes_text = format!("{}/{}", self.nodes, self.available_nodes);
            bar.update_field("nodes", &nodes_text).ok();

            // Update DHT info
            if let Some(stats) = &self.storage_stats {
                let dht_text = format!("{} realms, {} pages", stats.realm_count, stats.page_count);
                bar.update_field("dht", &dht_text).ok();

                // Update storage
                let storage_text = StatusBar::format_bytes(stats.total_size);
                bar.update_field("storage", &storage_text).ok();
            } else if !self.realms.is_empty() {
                let dht_text = format!("{} realms", self.realms.len());
                bar.update_field("dht", &dht_text).ok();
            }

            if let Some(ni) = &self.node_info {
                bar.update_node_info(&ni).ok();
            }

            bar.update_node_list(&self.connected_node_ids).ok();
            bar.update_connection(self.connected, self.connected_node_ids.len() > 0)
                .ok();
        }
    }

    fn dht_storage(&mut self, msg: DHTStorageOut) {
        match msg {
            DHTStorageOut::FloValue(item) => self.dht_storage(DHTStorageOut::FloValues(vec![item])),
            DHTStorageOut::FloValues(items) => self.emit_event(
                NodeStatus::ReceivedFlo,
                items
                    .into_iter()
                    .map(|fc| FloID::new(fc.0.flo_id()))
                    .collect::<Vec<_>>()
                    .into(),
            ),
            DHTStorageOut::RealmIDs(ids) => {
                self.realms = ids
                    .into_iter()
                    .map(|id| RealmID::new(id))
                    .collect::<Vec<_>>();
                if self.realms.len() > 0 {
                    self.emit_event(NodeStatus::RealmAvailable, self.realms.clone().into());
                }
                self.update_status_bar();
            }
            DHTStorageOut::Stats(stats) => {
                let mut total_size = 0;
                let mut page_count = 0;

                for (_, realm_stats) in stats.realm_stats.iter() {
                    total_size += realm_stats.real_size;
                    page_count += realm_stats.flos;
                }

                self.storage_stats = Some(StorageStats {
                    total_size,
                    realm_count: stats.realm_stats.len(),
                    page_count,
                });
                self.total_pages = page_count;

                self.update_status_bar();
            }
            _ => {}
        }
    }

    fn dht_router(&mut self, msg: DHTRouterOut) {
        match msg {
            DHTRouterOut::NodeList(ids) => {
                self.nodes = ids.len();

                // Convert NodeIDs to hex strings (first 12 chars)
                self.connected_node_ids = ids
                    .iter()
                    .map(|id| format!("{:x}", id).chars().take(12).collect())
                    .collect();

                self.emit_event(
                    if self.nodes > 0 {
                        NodeStatus::ConnectedNodes
                    } else {
                        NodeStatus::DisconnectNodes
                    },
                    ids.len().into(),
                );

                self.update_status_bar();
            }
            _ => {}
        }
    }

    fn network(&mut self, msg: NetworkOut) {
        match msg {
            NetworkOut::SystemConfig(_) => {
                self.connected = true;
                self.emit_event(NodeStatus::ConnectSignal, JsValue::null());
                self.update_status_bar();
            }
            NetworkOut::NodeListFromWS(list) => {
                self.available_nodes = list.len();
                self.emit_event(NodeStatus::AvailableNodes, list.len().into());
                self.update_status_bar();
            }
            _ => {}
        }
    }

    fn proxy(&mut self, msg: ProxyOut) {
        match msg {
            ProxyOut::Elected => {
                self.is_leader = true;
                self.emit_event(NodeStatus::IsLeader, JsValue::null());
            }
            ProxyOut::TabList(tab_ids) => {
                self.emit_event(NodeStatus::TabsCount, tab_ids.len().into());
            }
            _ => {}
        }
    }

    fn send_past(&self) {
        if self.connected {
            self.emit_event(NodeStatus::ConnectSignal, JsValue::null());
        }
        if self.nodes > 0 {
            self.emit_event(NodeStatus::ConnectedNodes, self.nodes.into());
        }
        if self.realms.len() > 0 {
            self.emit_event(NodeStatus::RealmAvailable, self.realms.clone().into());
        }
        if self.is_leader {
            self.emit_event(NodeStatus::IsLeader, JsValue::null());
        }
    }
}

#[platform_async_trait()]
impl SubsystemHandler<EventsIn, ()> for Events {
    async fn messages(&mut self, msgs: Vec<EventsIn>) -> Vec<()> {
        for msg in msgs {
            match msg {
                EventsIn::DhtStorage(out) => self.dht_storage(out),
                EventsIn::DhtRouter(out) => self.dht_router(out),
                EventsIn::Network(out) => self.network(out),
                EventsIn::EventHandler(function) => {
                    self.event_callback.lock().unwrap().replace(function);
                    self.send_past();
                }
                EventsIn::ClearEvent => {
                    self.event_callback.lock().unwrap().take();
                }
                EventsIn::Proxy(out) => self.proxy(out),
                EventsIn::SetStatusDiv(div_id) => match StatusBar::new(&div_id) {
                    Ok(sb) => {
                        self.status_bar = Some(sb);
                        self.update_status_bar();
                    }
                    Err(e) => log::error!("Couldn't create status bar: {e:?}"),
                },
                EventsIn::ClearStatusDiv => {
                    self.status_bar = None;
                }
                EventsIn::NodeInfo(ni) => {
                    self.node_info = Some(ni);
                    self.update_status_bar();
                }
            }
        }
        vec![]
    }
}

#[wasm_bindgen]
#[derive(Clone)]
pub enum NodeStatus {
    // Connection to the signalling server established
    ConnectSignal,
    // Number of nodes connected, always >= 1
    ConnectedNodes,
    // Available nodes on the signalling server
    AvailableNodes,
    // Lost connection to the signalling server - connection to the nodes
    // can still be available - currently not available
    DisconnectSignal,
    // Lost connection to last node - signal server might still be connected
    DisconnectNodes,
    // Realm available - this might happen before any connections are set up.
    // Sends the list of available realm-IDs
    RealmAvailable,
    // Got new Flo - not really sure if it is a new version, or just generally
    // a Flo arrived.
    ReceivedFlo,
    // This tab is the leader
    IsLeader,
    // Number of tabs available, in the data.
    TabsCount,
}
