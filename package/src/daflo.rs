//! DaFlo represents a Flo.
//! In it's initial state it's not necessarily initialized.

use flarch::tasks::spawn_local;
use flmodules::flo::{
    blob::{self, BlobPage},
    flo::FloID,
    realm::RealmID,
    realm_storage::RealmStorage,
};
use js_sys::Function;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use wasm_bindgen::{prelude::wasm_bindgen, JsValue};

use crate::{
    darealm::{DHTFetcherState, FloIDPath},
    error::WasmError,
    proxy::{
        proxy::{BrokerProxy, ProxyOut},
        state::{State, StateUpdate},
    },
};

#[wasm_bindgen]
pub struct FloState {
    broker: BrokerProxy,
    realm: RealmID,
    fip: FloIDPath,
    state: watch::Receiver<State>,
    version: Option<u32>,
}

#[wasm_bindgen]
impl FloState {
    pub async fn add_floblobpage_listener(
        &mut self,
        _callback: Function,
    ) -> Result<usize, WasmError> {
        // let (mut tap, id) = self
        //     .broker
        //     .get_tap_out()
        //     .await
        //     .map_err(|e| format!("Getting tap: {e:?}"))?;

        // let state = self.state.clone();
        // let fip = self.fip.clone();
        // let rs: RealmStorage<BlobPage> = RealmStorage::from_root(ds, root).await?;
        // if self.state.borrow().get_flo(&gid) {
        //     self.broker
        //         .emit_msg_out(ProxyOut::Update(StateUpdate::ReceivedFlo))
        //         .map_err(|e| format!("While injecting FloUpdate: {e:?}"))?;
        // }
        // let mut version = self.version.clone();
        spawn_local(async move {
            // while let Some(ProxyOut::Update(StateUpdate::ReceivedFlo)) = tap.recv().await {
            //     if let Some(flo) = state
            //         .borrow()
            //         .flos
            //         .get(gid.realm_id())
            //         .and_then(|flos| flos.get(gid.flo_id()))
            //         .and_then(|flo| blob::FloBlobPage::try_from(flo.0.clone()).ok())
            //     {
            //         if version != Some(flo.version()) {
            //             version = Some(flo.version());
            //             if let Err(e) = callback.call1(&JsValue::null(), &FloBlobPage(flo).into()) {
            //                 log::error!("Couldn't call state listener: {e:?}");
            //             }
            //         }
            //     }
            // }
        });

        // Ok(id)
        todo!()
    }

    pub async fn remove_floblobpage_listener(&mut self, id: usize) -> Result<(), WasmError> {
        Ok(self.broker.remove_subsystem(id).await?)
    }
}

impl From<&FloState> for DHTFetcherState {
    fn from(fs: &FloState) -> Self {
        DHTFetcherState {
            broker: fs.broker.clone(),
            state: fs.state.clone(),
        }
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloBlobPage(blob::FloBlobPage);

#[wasm_bindgen]
impl FloBlobPage {
    pub fn get_index(&self) -> String {
        self.0.get_index()
    }
}

impl FloState {
    pub fn from_id(
        broker: BrokerProxy,
        state: watch::Receiver<State>,
        realm: RealmID,
        flo: FloID,
    ) -> Self {
        Self {
            broker,
            realm,
            fip: FloIDPath::ID(flo),
            state,
            version: None,
        }
    }
    pub fn from_path(
        broker: BrokerProxy,
        state: watch::Receiver<State>,
        realm: RealmID,
        path: String,
    ) -> Self {
        Self {
            broker,
            realm,
            fip: FloIDPath::Path(path),
            state,
            version: None,
        }
    }
}
