//! Handels realms in an asynchronous way.
//! In Danu, realms are not guaranteed to be available right away, and updates
//! to the realms can come in at any time.
//! So while there is a local storage, there are no methods
//! to get a specific realm or a specific Flo from a realm,
//! only callbacks to listen to new realms.

use std::collections::{HashMap, HashSet};

use flarch::{
    add_translator,
    broker::{Broker, SubsystemHandler},
    platform_async_trait,
};
use flmodules::{
    dht_storage::{broker::DHTStorageIn, core::FloCuckoo},
    flo::{
        blob::{self, BlobFamily, BlobPath},
        flo::{self, Flo, FloWrapper},
        realm::{GlobalID, Realm, RealmID},
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
    intern: BrokerIntern,
    _realm: RealmID,
    _state: watch::Receiver<State>,
}

impl RealmObserver {
    pub async fn start(
        mut broker: BrokerProxy,
        state: watch::Receiver<State>,
        id: RealmID,
    ) -> anyhow::Result<RealmObserver> {
        let mut intern = Intern::start(state.clone(), id.clone()).await?;
        add_translator!(broker, o_ti, intern, ProxyOut::Update(msg) => InternIn::State(msg));
        add_translator!(intern, o_ti, broker, InternOut::Node(msg) => ProxyIn::Node(msg));
        Ok(RealmObserver {
            intern,
            _realm: id,
            _state: state,
        })
    }
}

#[wasm_bindgen]
impl RealmObserver {
    pub async fn get_flo_page_id(&mut self, id: FloID) -> Result<ReadableStream, WasmError> {
        self.get_flo::<FloBlobPage>(FloIDPath::ID(id.into())).await
    }

    pub async fn get_flo_page_path(&mut self, path: String) -> Result<ReadableStream, WasmError> {
        self.get_flo::<FloBlobPage>(FloIDPath::Path(path)).await
    }
}

impl RealmObserver {
    async fn get_flo<T: std::fmt::Debug + Clone + Serialize + DeserializeOwned + 'static>(
        &mut self,
        fip: FloIDPath,
    ) -> Result<ReadableStream, WasmError> {
        // TODO: Add different FloWrapper types like FloBlobPage, FloBlobTag, FloBlobRealm
        let id_cell = std::rc::Rc::new(std::cell::Cell::new(0usize));
        let id_cell_clone = id_cell.clone();
        let (stream, id) = self.intern
            .get_async_iterable_out_translate::<flo::FloWrapper<T>>(Box::new(move |msg| match msg {
                InternOut::Stream(out_id, fbp) if out_id == id_cell_clone.get() => fbp.try_into().ok(),
                _ => None,
            }))
            .await?;
        id_cell.set(id);
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
    Stream(usize, flo::Flo),
    Node(NodeIn),
}

#[derive(Debug)]
struct Intern {
    fips: HashMap<usize, FloIDPath>,
    state: watch::Receiver<State>,
    rid: RealmID,
    request: HashSet<GlobalID>,
}

impl Intern {
    async fn start(state: watch::Receiver<State>, rid: RealmID) -> anyhow::Result<BrokerIntern> {
        Ok(Broker::new_with_handler(Box::new(Intern {
            fips: HashMap::new(),
            state,
            rid,
            request: HashSet::new(),
        }))
        .await?
        .0)
    }

    fn get_flo(&mut self, gid: &GlobalID) -> Option<Flo> {
        self.request.insert(gid.clone());
        self.get_flo_cuckoos(gid).map(|(flo, _)| flo)
    }

    fn get_flo_wrapper<T: std::fmt::Debug + Clone + Serialize + DeserializeOwned>(
        &mut self,
        gid: &GlobalID,
    ) -> anyhow::Result<FloWrapper<T>> {
        self.get_flo_cuckoos(gid)
            .map(|(flo, _)| FloWrapper::<T>::try_from(flo))
            .unwrap_or(Err(anyhow::anyhow!("didn't find flo")))
    }

    fn get_flo_cuckoos(&mut self, gid: &GlobalID) -> Option<FloCuckoo> {
        self.request.insert(gid.clone());
        self.state
            .borrow()
            .flos
            .get(gid.realm_id())
            .and_then(|realm| realm.get(gid.flo_id()))
            .cloned()
    }

    fn get_flo_path(&mut self, path: &str) -> anyhow::Result<Flo> {
        let Some((service, path)) = path.split_once("://") else {
            anyhow::bail!("Invalid path format, expected 'service://path'");
        };
        let flo_realm = self.get_flo_wrapper::<Realm>(&self.rid.global_id((*self.rid).into()))?;
        let root = flo_realm
            .cache()
            .get_services()
            .get(service)
            .ok_or_else(|| anyhow::anyhow!("Service not found"))?;
        let parts = path.trim_start_matches('/').split('/').collect::<Vec<_>>();
        self.get_flo_path_internal(&parts, &[root.clone()])
    }

    fn get_flo_path_internal(
        &mut self,
        parts: &[&str],
        curr_ids: &[flo::FloID],
    ) -> anyhow::Result<Flo> {
        let ids: Vec<flo::FloID> = curr_ids
            .iter()
            .flat_map(|id| {
                std::iter::once(id.clone()).chain(
                    self.get_flo_cuckoos(&self.rid.global_id(id.clone()))
                        .map(|(_, cuckoos)| cuckoos.clone())
                        .unwrap_or_default(),
                )
            })
            .collect();

        if let Some((part, rest)) = parts.split_first() {
            for id in &ids {
                if let Some(flo) = self.get_flo(&self.rid.global_id(id.clone())) {
                    if let Some((path, children)) = self.get_flo_blob_path_children(&flo) {
                        if &path == part {
                            if rest.is_empty() {
                                return Ok(flo.clone());
                            } else if !children.is_empty() {
                                return self.get_flo_path_internal(rest, &children);
                            }
                        }
                    }
                }
            }
        }

        Err(anyhow::anyhow!("Couldn't find path"))
    }

    fn get_flo_blob_path_children(&self, flo: &Flo) -> Option<(String, Vec<flo::FloID>)> {
        if let Ok(blob) = blob::FloBlob::try_from(flo.clone()) {
            let path = blob.get_path()?.to_string();
            let children = blob
                .get_children()
                .into_iter()
                .map(|b| (*b).into())
                .collect();
            return Some((path, children));
        }
        if let Ok(blob) = blob::FloBlobPage::try_from(flo.clone()) {
            let path = blob.get_path()?.to_string();
            let children = blob
                .get_children()
                .into_iter()
                .map(|b| (*b).into())
                .collect();
            return Some((path, children));
        }
        if let Ok(blob) = blob::FloBlobTag::try_from(flo.clone()) {
            let path = blob.get_path()?.to_string();
            let children = blob
                .get_children()
                .into_iter()
                .map(|b| (*b).into())
                .collect();
            return Some((path, children));
        }
        None
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
                    let mut updates = vec![];
                    let fips = self.fips.clone();
                    for (id, fip) in fips.iter() {
                        match fip {
                            FloIDPath::ID(fid) => {
                                if fid == &f {
                                    if let Some(flo) =
                                        self.get_flo(&self.rid.global_id(fid.clone()))
                                    {
                                        out.push(InternOut::Stream(*id, flo))
                                    }
                                }
                            }
                            FloIDPath::Path(path) => match self.get_flo_path(path) {
                                Err(e) => log::error!("Couldn't get flo path: {e:?}"),
                                Ok(flo) => {
                                    updates.push((*id, flo.flo_id()));
                                    out.push(InternOut::Stream(*id, flo));
                                }
                            },
                        }
                    }
                    for update in updates {
                        self.fips.insert(update.0, FloIDPath::ID(update.1));
                    }
                }
                _ => {}
            }
        }
        for id in self.request.drain() {
            out.push(InternOut::Node(NodeIn::DHTStorage(DHTStorageIn::ReadFlo(
                id,
            ))));
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

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use bytes::Bytes;
    use flarch::nodeids::U256;
    use flcrypto::{access::Condition, signer::SignerTrait, signer_ed25519::SignerEd25519};
    use flmodules::{
        dht_storage::core::{FloCuckoo, RealmConfig},
        flo::{
            blob::{BlobFamily, FloBlobPage},
            flo::FloID,
            realm::{FloRealm, RealmID},
        },
        nodeconfig,
    };
    use tokio::sync::watch;
    use wasm_bindgen_test::wasm_bindgen_test;

    use crate::proxy::state::State;

    use super::Intern;

    fn make_state(realm_id: RealmID, flos: Vec<(FloID, FloCuckoo)>) -> State {
        let mut realm_map = HashMap::new();
        for (fid, fc) in flos {
            realm_map.insert(fid, fc);
        }
        let mut flos_map = HashMap::new();
        flos_map.insert(realm_id, realm_map);

        State {
            config: None,
            node_info: nodeconfig::NodeInfo::new_from_id(U256::rnd()),
            realm_ids: vec![],
            nodes_connected_dht: vec![],
            nodes_online: vec![],
            dht_storage_stats: Default::default(),
            flos: flos_map,
            is_leader: None,
            tab_list: vec![],
        }
    }

    #[wasm_bindgen_test]
    fn test_get_flo_path() -> anyhow::Result<()> {
        let signer = SignerEd25519::new();
        let cond = Condition::Verifier(signer.verifier());

        // Create the realm
        let fr = FloRealm::new(
            "test-realm",
            cond.clone(),
            RealmConfig {
                max_space: 1_000_000,
                max_flo_size: 10_000,
            },
            &[&signer],
        )?;
        let realm_id = fr.realm_id();

        // Create the root page for the html service (path: "root")
        let root_page = FloBlobPage::new(
            realm_id.clone(),
            cond.clone(),
            "root",
            Bytes::from("<html>root</html>"),
            None,
            &[&signer],
        )?;
        let root_id = root_page.flo_id();

        // Create a child page (path: "about")
        let about_page = FloBlobPage::new(
            realm_id.clone(),
            cond.clone(),
            "about",
            Bytes::from("<html>about</html>"),
            None,
            &[&signer],
        )?;
        let about_id = about_page.flo_id();

        // Add about_page as a blob child of root_page
        let root_page = root_page.edit_data_signers(
            cond.clone(),
            |bp| bp.add_child(about_page.blob_id()),
            &[&signer],
        )?;

        // Wire realm: register html service pointing at root_page
        let fr = fr.edit_data_signers(
            cond.clone(),
            |r| r.set_service("html", root_id.clone()),
            &[&signer],
        )?;

        // Build FloCuckoo tuples: (Flo, Vec<FloID>)
        let realm_flo = fr.flo().clone();
        let realm_fid = realm_flo.flo_id();
        let root_flo = root_page.flo().clone();
        let about_flo = about_page.flo().clone();

        let flos = vec![
            (realm_fid.clone(), (realm_flo, vec![])),
            (root_id.clone(), (root_flo, vec![])),
            (about_id.clone(), (about_flo, vec![])),
        ];

        let state = make_state(realm_id.clone(), flos);
        let (tx, rx) = watch::channel(state);
        let _ = tx; // keep sender alive

        let mut intern = Intern {
            fips: HashMap::new(),
            state: rx,
            rid: realm_id.clone(),
            request: Default::default(),
        };

        // Test: resolving "html://root" should return the root page
        let flo = intern.get_flo_path("html://root")?;
        assert_eq!(flo.flo_id(), root_id);

        // Test: resolving "html://root/about" should return the about page
        let flo2 = intern.get_flo_path("html://root/about")?;
        assert_eq!(flo2.flo_id(), about_id);

        Ok(())
    }
}
