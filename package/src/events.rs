use std::sync::Mutex;

use flarch::{
    broker::{Broker, SubsystemHandler},
    platform_async_trait,
};
use flmodules::{
    dht_router::broker::DHTRouterOut, dht_storage::broker::DHTStorageOut,
    network::broker::NetworkOut,
};

use js_sys::Function;
use wasm_bindgen::prelude::*;

use crate::{
    darealm::{FloID, RealmID},
    proxy::proxy::Proxy,
};

#[derive(Debug, Clone)]
pub enum EventsIn {
    DhtStorage(DHTStorageOut),
    DhtRouter(DHTRouterOut),
    Network(NetworkOut),
    Event(Function),
    ClearEvent,
}

pub type BrokerEvents = Broker<EventsIn, ()>;

pub struct Events {
    event_callback: Mutex<Option<Function>>,
    connected: bool,
    realms: Vec<RealmID>,
    nodes: usize,
}

impl Events {
    pub async fn new(proxy: &mut Proxy) -> Result<BrokerEvents, anyhow::Error> {
        let b = Broker::new_with_handler(Box::new(Events {
            event_callback: Mutex::new(None),
            connected: false,
            realms: vec![],
            nodes: 0,
        }))
        .await?
        .0;
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
        proxy
            .dht_storage
            .emit_msg_in(flmodules::dht_storage::broker::DHTStorageIn::GetRealms)?;
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
            }
            _ => {}
        }
    }

    fn dht_router(&mut self, msg: DHTRouterOut) {
        match msg {
            DHTRouterOut::NodeList(ids) => {
                self.nodes = ids.len();
                self.emit_event(
                    if self.nodes > 0 {
                        NodeStatus::ConnectedNodes
                    } else {
                        NodeStatus::DisconnectNodes
                    },
                    ids.len().into(),
                );
            }
            _ => {}
        }
    }

    fn network(&mut self, msg: NetworkOut) {
        match msg {
            NetworkOut::SystemConfig(_) => {
                self.connected = true;
                self.emit_event(NodeStatus::ConnectSignal, JsValue::null())
            }
            NetworkOut::NodeListFromWS(list) => {
                self.emit_event(NodeStatus::AvailableNodes, list.len().into());
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
                EventsIn::Event(function) => {
                    self.event_callback.lock().unwrap().replace(function);
                    self.send_past();
                }
                EventsIn::ClearEvent => {
                    self.event_callback.lock().unwrap().take();
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
}
