//! Handels realms in an asynchronous way.
//! In Danu, realms are not guaranteed to be available right away, and updates
//! to the realms can come in at any time.
//! So while there is a local storage, there are no methods
//! to get a specific realm or a specific Flo from a realm,
//! only callbacks to listen to new realms.

use std::collections::HashMap;

use async_trait::async_trait;
use flarch::{
    add_translator,
    broker::{Broker, SubsystemHandler},
    platform_async_trait,
};
use flmodules::{
    dht_storage::broker::DHTStorageIn,
    flo::{
        flo::{self, FloWrapper},
        realm::{GlobalID, RealmID},
        realm_storage::DHTFetcher,
    },
};
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::watch;
use wasm_bindgen::prelude::wasm_bindgen;
use web_sys::ReadableStream;

use crate::{
    daflo::FloBlobPage,
    error::WasmError,
    ids::FloID,
    proxy::{
        proxy::{BrokerProxy, NodeIn, ProxyIn, ProxyOut},
        state::{State, StateUpdate},
    },
};

#[wasm_bindgen]
#[derive(Debug)]
pub struct RealmObserver {
    broker: BrokerProxy,
    intern: BrokerIntern,
    realm: RealmID,
    state: watch::Receiver<State>,
}

impl RealmObserver {
    pub async fn start(
        mut broker: BrokerProxy,
        state: watch::Receiver<State>,
        id: RealmID,
    ) -> anyhow::Result<RealmObserver> {
        let intern = Intern::start(state.clone(), id.clone()).await?;
        add_translator!(broker, o_ti, intern, ProxyOut::Update(msg) => InternIn::State(msg));
        Ok(RealmObserver {
            broker,
            intern,
            realm: id,
            state,
        })
    }
}

#[wasm_bindgen]
impl RealmObserver {
    pub async fn get_flo_id(&mut self, id: FloID) -> Result<ReadableStream, WasmError> {
        self.get_flo(FloIDPath::ID(id.into())).await
    }

    pub async fn get_flo_path(&mut self, path: String) -> Result<ReadableStream, WasmError> {
        self.get_flo(FloIDPath::Path(path)).await
    }
}

impl RealmObserver {
    async fn get_flo(&mut self, fip: FloIDPath) -> Result<ReadableStream, WasmError> {
        // TODO: Add different FloWrapper types like FloBlobPage, FloBlobTag, FloBlobRealm
        let mut b: Broker<(), flo::Flo> = Broker::new();
        let (stream, id) = b.get_async_iterable_out().await?;
        self.intern
            .add_translator_o_to(
                b,
                Box::new(move |msg| match msg {
                    InternOut::Out(out_id, fbp) if out_id == id => Some(fbp),
                    _ => None,
                }),
            )
            .await?;
        self.intern.emit_msg_in(InternIn::Stream(id, fip))?;
        Ok(stream)
    }
}

type BrokerIntern = Broker<InternIn, InternOut>;

#[derive(Debug, Clone)]
enum InternIn {
    State(StateUpdate),
    Stream(usize, FloIDPath),
}

#[derive(Debug, Clone)]
enum InternOut {
    Out(usize, flo::Flo),
}

#[derive(Debug)]
struct Intern {
    fips: HashMap<usize, FloIDPath>,
    state: watch::Receiver<State>,
    rid: RealmID,
}

impl Intern {
    async fn start(state: watch::Receiver<State>, rid: RealmID) -> anyhow::Result<BrokerIntern> {
        Ok(Broker::new_with_handler(Box::new(Intern {
            fips: HashMap::new(),
            state,
            rid,
        }))
        .await?
        .0)
    }
}

#[platform_async_trait]
impl SubsystemHandler<InternIn, InternOut> for Intern {
    async fn messages(&mut self, msgs: Vec<InternIn>) -> Vec<InternOut> {
        let mut out = vec![];
        for msg in msgs {
            match msg {
                InternIn::Stream(id, fip) => {
                    self.fips.insert(id, fip);
                }
                InternIn::State(StateUpdate::ReceivedFlo(f)) => {
                    // let mut updates = vec![];
                    for (id, fip) in self.fips.iter() {
                        match fip {
                            FloIDPath::ID(fid) => {
                                if fid == &f {
                                    if let Some(flo) = self
                                        .state
                                        .borrow()
                                        .get_flo(&self.rid.global_id(fid.clone()))
                                    {
                                        out.push(InternOut::Out(*id, flo))
                                    }
                                }
                            }
                            FloIDPath::Path(path) => {
                                // if let Some(flo) = self.state.borrow().get_flo_path(path) {
                                //     updates.push((*id, flo.get_id()));
                                //     out.push(InternOut::Out(*id, flo));
                                // }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FloIDPath {
    ID(flo::FloID),
    Path(String),
}

impl FloIDPath {
    pub fn get_id(&self) -> Option<flo::FloID> {
        match self {
            FloIDPath::ID(flo_id) => Some(flo_id.clone()),
            FloIDPath::Path(_) => None,
        }
    }

    pub fn get_path(&self) -> Option<String> {
        match self {
            FloIDPath::ID(_) => None,
            FloIDPath::Path(path) => Some(path.clone()),
        }
    }
}

#[derive(Debug)]
pub struct DHTFetcherState {
    pub broker: BrokerProxy,
    pub state: watch::Receiver<State>,
}

impl From<&RealmObserver> for DHTFetcherState {
    fn from(rs: &RealmObserver) -> Self {
        DHTFetcherState {
            broker: rs.broker.clone(),
            state: rs.state.clone(),
        }
    }
}

#[async_trait(?Send)]
impl<T: Serialize + DeserializeOwned + Clone> DHTFetcher<T> for DHTFetcherState {
    async fn get_cuckoos(&mut self, id: &GlobalID) -> anyhow::Result<Vec<flo::FloID>> {
        self.broker.emit_msg_in(ProxyIn::Node(NodeIn::DHTStorage(
            DHTStorageIn::ReadCuckooIDs(id.clone()),
        )))?;
        Ok(self
            .state
            .borrow()
            .get_flo_cuckoos(id)
            .map(|(_, ids)| ids.clone())
            .unwrap_or(vec![]))
    }

    async fn get_flo(&mut self, id: &GlobalID) -> anyhow::Result<FloWrapper<T>> {
        self.broker.emit_msg_in(ProxyIn::Node(NodeIn::DHTStorage(
            DHTStorageIn::ReadCuckooIDs(id.clone()),
        )))?;
        self.state
            .borrow()
            .get_flo(id)
            .ok_or(anyhow::anyhow!("Didn't find flo"))
            .and_then(|flo| flo.try_into())
    }
}
