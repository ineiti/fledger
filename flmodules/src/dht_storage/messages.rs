use std::{collections::HashMap, iter};

use flarch::{
    broker::SubsystemHandler,
    data_storage::DataStorage,
    nodeids::{NodeID, U256},
    platform_async_trait,
};
use flcrypto::tofrombytes::ToFromBytes;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::{
    dht_router::broker::{DHTRouterIn, DHTRouterOut},
    flo::{
        flo::{Flo, FloID},
        realm::{FloRealm, GlobalID, RealmID},
    },
    router::messages::NetworkWrapper,
};

use super::{
    broker::{DHTStorageIn, DHTStorageOut, StorageError, MODULE_NAME},
    core::*,
};

/// The messages here represent all possible interactions with this module.
#[derive(Debug, Clone, PartialEq)]
pub enum InternIn {
    Routing(DHTRouterOut),
    Storage(DHTStorageIn),
    BroadcastSync,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InternOut {
    Routing(DHTRouterIn),
    Storage(DHTStorageOut),
}

/// These messages are sent to the closest node given in the DHTRouting message.
/// Per default, the 'key' value of the DHTRouting will be filled with the FloID.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageNodeClosest {
    // Stores the given Flo in the closest node, and all nodes on the route
    // which have enough place left.
    StoreFlo(Flo),
    // Request a Flo. The FloID is in the DHTRouting::Request.
    ReadFlo(RealmID),
    // Request Cuckoos for the given ID. The FloID is in the DHTRouting::Request.
    GetCuckooIDs(RealmID),
    // Store the Cuckoo-ID in the relevant Flo. The DHTRouting::Request(key) is the
    // parent Flo, and the given GlobalID is the ID of the Cuckoo and the RealmID of the
    // parent Flo and the Cuckoo.
    StoreCuckooID(GlobalID),
}

/// These messages are sent directly to the requester of the MessageNodeClosest.
/// As in this case there is no 'key', the IDs need to be transmitted completely.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageNodeDirect {
    // Returns the result of the requested Flo, including any available Cuckoo-IDs.
    FloValue(FloCuckoo),
    // Indicates this Flo is not in the closest node.
    UnknownFlo(GlobalID),
    // The Cuckoo-IDs stored next to the Flo composed of the GlobalID
    CookooIDs(GlobalID, Vec<FloID>),
    // The update protocol
    Update(Sync),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageBroadcast {
    // Asks all nodes to sync with the source node.
    Sync,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Sync {
    RequestRealmIDs,
    RequestFloMetas(RealmID),
    AvailableRealmIDs(Vec<RealmID>),
    AvailableFlos(RealmID, Vec<FloMeta>),
    RequestFlos(RealmID, Vec<FloID>),
    Flos(Vec<FloCuckoo>),
}

#[derive(Debug, Default)]
pub struct Stats {
    pub realm_sizes: HashMap<RealmID, usize>,
}

/// The message handling part, but only for DHTStorage messages.
pub struct Messages {
    realms: HashMap<RealmID, RealmStorage>,
    nodes: Vec<NodeID>,
    config: DHTConfig,
    our_id: NodeID,
    ds: Box<dyn DataStorage + Send>,
    tx: Option<watch::Sender<Stats>>,
}

impl Messages {
    /// Returns a new chat module.
    pub fn new(
        ds: Box<dyn DataStorage + Send>,
        config: DHTConfig,
        our_node: NodeID,
    ) -> (Self, watch::Receiver<Stats>) {
        let str = ds.get(MODULE_NAME).unwrap_or("".into());
        let realms = serde_yaml::from_str(&str).unwrap_or(HashMap::new());
        let (tx, rx) = watch::channel(Stats::default());
        (
            Self {
                realms,
                config,
                our_id: our_node,
                nodes: vec![],
                ds,
                tx: Some(tx),
            },
            rx,
        )
    }

    fn msg_in_routing(&mut self, msg: DHTRouterOut) -> Vec<InternOut> {
        match msg {
            DHTRouterOut::MessageRouting(origin, _last_hop, _next_hop, key, msg) => msg
                .unwrap_yaml(MODULE_NAME)
                .map(|mn| self.msg_routing(false, origin, key, mn)),
            DHTRouterOut::MessageClosest(origin, _last_hop, key, msg) => msg
                .unwrap_yaml(MODULE_NAME)
                .map(|mn| self.msg_routing(true, origin, key, mn)),
            DHTRouterOut::MessageDest(origin, _last_hop, msg) => msg
                .unwrap_yaml(MODULE_NAME)
                .map(|mn| self.msg_dest(origin, mn)),
            DHTRouterOut::NodeList(nodes) => {
                self.nodes = nodes;
                None
            }
            DHTRouterOut::MessageBroadcast(origin, msg) => msg
                .unwrap_yaml(MODULE_NAME)
                .map(|mn| self.msg_broadcast(origin, mn)),
        }
        .unwrap_or(vec![])
    }

    fn msg_in_storage(&mut self, msg: DHTStorageIn) -> Vec<InternOut> {
        // log::warn!("Storing {msg:?}");
        match msg {
            DHTStorageIn::StoreFlo(flo) => self.store_flo(flo),
            DHTStorageIn::ReadFlo(id) => vec![match self.read_flo(&id) {
                Some(df) => DHTStorageOut::FloValue(df.clone()).into(),
                None => MessageNodeClosest::ReadFlo(id.realm_id().clone())
                    .to_intern_out(id.flo_id().clone().into())
                    .expect("Creating ReadFlo message"),
            }],
            DHTStorageIn::ReadCuckooIDs(id) => {
                let mut out: Vec<InternOut> = self
                    .realms
                    .get(id.realm_id())
                    .and_then(|realm| realm.get_cuckoo_ids(id.flo_id()))
                    .map(|cids| vec![DHTStorageOut::CuckooIDs(id.clone(), cids).into()])
                    .unwrap_or_default();
                out.push(
                    MessageNodeClosest::GetCuckooIDs(id.realm_id().clone())
                        .to_intern_out(id.flo_id().clone().into())
                        .expect("Creating GetCuckoos"),
                );
                out
            }
            DHTStorageIn::SyncNeighbors => self
                .nodes
                .iter()
                .flat_map(|n| {
                    iter::once(Sync::RequestRealmIDs)
                        .chain(
                            self.realms
                                .keys()
                                .map(|rid| Sync::RequestFloMetas(rid.clone())),
                        )
                        .filter_map(|msg| MessageNodeDirect::Update(msg).to_intern_out(*n))
                        .collect::<Vec<_>>()
                })
                .collect(),
            DHTStorageIn::GetRealms => {
                vec![DHTStorageOut::RealmIDs(self.realms.keys().cloned().collect()).into()]
            }
        }
    }

    fn msg_routing(
        &mut self,
        _closest: bool,
        origin: NodeID,
        key: U256,
        msg: MessageNodeClosest,
    ) -> Vec<InternOut> {
        let fid: FloID = key.into();
        match msg {
            MessageNodeClosest::StoreFlo(flo) => {
                return self.store_flo(flo);
            }
            MessageNodeClosest::ReadFlo(rid) => {
                if let Some(fc) = self
                    .realms
                    .get(&rid)
                    .and_then(|realm| realm.get_flo_cuckoo(&fid))
                {
                    return MessageNodeDirect::FloValue(fc)
                        .to_intern_out(origin)
                        .map_or(vec![], |msg| vec![msg]);
                }
            }
            MessageNodeClosest::GetCuckooIDs(rid) => {
                let parent = GlobalID::new(rid.clone(), fid.clone());
                return self
                    .realms
                    .get(&rid)
                    .and_then(|realm| realm.get_cuckoo_ids(&fid))
                    .map_or(vec![], |ids| {
                        MessageNodeDirect::CookooIDs(parent, ids)
                            .to_intern_out(origin)
                            .map_or(vec![], |msg| vec![msg])
                    });
            }
            MessageNodeClosest::StoreCuckooID(gid) => {
                self.realms
                    .get_mut(&gid.realm_id())
                    .map(|realm| realm.store_cuckoo_id(&fid, gid.flo_id().clone()));
            }
        }
        vec![]
    }

    fn msg_dest(&mut self, origin: NodeID, msg: MessageNodeDirect) -> Vec<InternOut> {
        match msg {
            MessageNodeDirect::FloValue(flo) => {
                self.store_flo(flo.0.clone());
                Some(DHTStorageOut::FloValue(flo).into())
            }
            MessageNodeDirect::UnknownFlo(key) => Some(DHTStorageOut::ValueMissing(key).into()),
            MessageNodeDirect::Update(sync) => return self.msg_sync(origin, sync),
            MessageNodeDirect::CookooIDs(gid, cids) => {
                Some(DHTStorageOut::CuckooIDs(gid, cids).into())
            }
        }
        .map_or(vec![], |msg| vec![msg])
    }

    fn msg_broadcast(&mut self, origin: NodeID, msg: MessageBroadcast) -> Vec<InternOut> {
        match msg {
            MessageBroadcast::Sync => iter::once(Sync::RequestRealmIDs)
                .chain(
                    self.realms
                        .keys()
                        .map(|rid| Sync::RequestFloMetas(rid.clone())),
                )
                .filter_map(|msg| MessageNodeDirect::Update(msg).to_intern_out(origin))
                .collect::<Vec<_>>(),
        }
    }

    fn msg_sync(&mut self, origin: NodeID, msg: Sync) -> Vec<InternOut> {
        // log::debug!("{} syncs {:?}", self.our_id, msg);
        match msg {
            Sync::RequestRealmIDs => vec![Sync::AvailableRealmIDs(
                self.realms.keys().cloned().collect(),
            )],
            Sync::RequestFloMetas(realm_id) => self
                .realms
                .get(&realm_id)
                .map(|realm| realm.get_flo_metas())
                .map_or(vec![], |fm| vec![Sync::AvailableFlos(realm_id, fm)]),
            Sync::AvailableRealmIDs(realm_ids) => realm_ids
                .into_iter()
                .filter(|rid| !self.realms.contains_key(&rid) && self.config.accepts_realm(&rid))
                .map(|rid| Sync::RequestFlos(rid.clone(), vec![(*rid).into()]))
                .collect(),
            Sync::AvailableFlos(realm_id, flo_metas) => self
                .realms
                .get(&realm_id)
                .and_then(|realm| realm.sync_available(&flo_metas))
                .map_or(vec![], |needed| vec![Sync::RequestFlos(realm_id, needed)]),
            Sync::RequestFlos(realm_id, flo_ids) => self
                .realms
                .get(&realm_id)
                .map(|realm| {
                    flo_ids
                        .iter()
                        .filter_map(|id| realm.get_flo_cuckoo(id))
                        .collect::<Vec<_>>()
                })
                .map_or(vec![], |flos| vec![Sync::Flos(flos)]),
            Sync::Flos(flo_cuckoos) => {
                for (flo, cuckoos) in flo_cuckoos {
                    self.store_flo(flo.clone());
                    self.realms.get_mut(&flo.realm_id()).map(|realm| {
                        cuckoos
                            .into_iter()
                            .for_each(|cuckoo| realm.store_cuckoo_id(&flo.flo_id(), cuckoo))
                    });
                }
                vec![]
            }
        }
        .into_iter()
        .filter_map(|msg| msg.to_intern_out(origin))
        .collect()
    }

    fn read_flo(&self, id: &GlobalID) -> Option<FloCuckoo> {
        self.realms
            .get(id.realm_id())
            .and_then(|realm| realm.get_flo_cuckoo(id.flo_id()))
    }

    fn store_flo(&mut self, flo: Flo) -> Vec<InternOut> {
        let mut res = vec![];
        if self.upsert_flo(flo.clone()) {
            res.extend(vec![MessageNodeClosest::StoreFlo(flo.clone())
                .to_intern_out(flo.flo_id().into())
                .expect("Storing new DHT")]);
        }
        if let Some(parent) = flo.flo_config().cuckoo_parent() {
            res.extend(vec![MessageNodeClosest::StoreCuckooID(flo.global_id())
                .to_intern_out(*parent.clone())
                .expect("Storing new Cuckoo")])
        }
        res
    }

    // Try really hard to store the flo.
    // Either its realm is already known, or it is a new realm.
    // When 'true' is returned, then the flo has been stored.
    fn upsert_flo(&mut self, flo: Flo) -> bool {
        // log::trace!("{} store_flo {flo:?}", self.our_id);
        let modification = self
            .realms
            .get_mut(&flo.realm_id())
            .map(|dsc| dsc.upsert_flo(flo.clone()))
            .unwrap_or_else(|| {
                TryInto::<FloRealm>::try_into(flo)
                    .ok()
                    .and_then(|realm| self.create_realm(realm).ok())
                    .is_some()
            });

        if modification {
            self.store();
        }
        modification
    }

    fn create_realm(&mut self, realm: FloRealm) -> Result<(), CoreError> {
        log::debug!("{} creating realm {}", self.our_id, realm.realm_id());
        if !self.config.accepts_realm(&realm.realm_id()) {
            return Err(CoreError::RealmNotAccepted);
        }
        let id = realm.flo().realm_id();
        let dsc = RealmStorage::new(self.config.clone(), self.our_id, realm)?;
        self.realms.insert(id, dsc);
        Ok(())
    }

    fn store(&mut self) {
        self.tx.clone().map(|tx| {
            tx.send(Stats::from_realms(&self.realms))
                .is_err()
                .then(|| self.tx = None)
        });
        serde_yaml::to_string(&self.realms)
            .ok()
            .map(|s| (*self.ds).set(MODULE_NAME, &s));
    }
}

#[platform_async_trait()]
impl SubsystemHandler<InternIn, InternOut> for Messages {
    async fn messages(&mut self, inputs: Vec<InternIn>) -> Vec<InternOut> {
        let _id = self.our_id;
        inputs
            .into_iter()
            // .inspect(|msg| log::debug!("{_id}: In: {msg:?}"))
            .flat_map(|msg| match msg {
                InternIn::Routing(dhtrouting_out) => __self.msg_in_routing(dhtrouting_out),
                InternIn::Storage(dhtstorage_in) => __self.msg_in_storage(dhtstorage_in),
                InternIn::BroadcastSync => MessageBroadcast::Sync
                    .try_into()
                    .map(|dht| vec![InternOut::Routing(dht)])
                    .unwrap_or(vec![]),
            })
            // .inspect(|msg| log::debug!("{_id}: Out: {msg:?}"))
            .collect()
    }
}

impl Stats {
    fn from_realms(realms: &HashMap<RealmID, RealmStorage>) -> Self {
        Self {
            realm_sizes: realms.iter().map(|(k, v)| (k.clone(), v.size())).collect(),
        }
    }
}

impl MessageNodeClosest {
    fn to_intern_out(&self, dst: NodeID) -> Option<InternOut> {
        NetworkWrapper::wrap_yaml(MODULE_NAME, self)
            .ok()
            .map(|msg_wrap| InternOut::Routing(DHTRouterIn::MessageClosest(dst, msg_wrap)))
    }
}

impl MessageNodeDirect {
    fn to_intern_out(&self, dst: NodeID) -> Option<InternOut> {
        NetworkWrapper::wrap_yaml(MODULE_NAME, self)
            .ok()
            .map(|msg_wrap| InternOut::Routing(DHTRouterIn::MessageDirect(dst, msg_wrap)))
    }
}

impl TryInto<DHTRouterIn> for MessageBroadcast {
    type Error = StorageError;

    fn try_into(self) -> Result<DHTRouterIn, Self::Error> {
        Ok(DHTRouterIn::MessageBroadcast(NetworkWrapper::wrap_yaml(
            MODULE_NAME,
            &self,
        )?))
    }
}

impl From<DHTStorageOut> for InternOut {
    fn from(value: DHTStorageOut) -> Self {
        InternOut::Storage(value)
    }
}

impl Sync {
    fn to_intern_out(self, dst: NodeID) -> Option<InternOut> {
        (match &self {
            Sync::AvailableRealmIDs(realm_ids) => realm_ids.len(),
            Sync::AvailableFlos(_, flo_metas) => flo_metas.len(),
            Sync::RequestFlos(_, flo_ids) => flo_ids.len(),
            Sync::Flos(items) => items.len(),
            _ => 1,
        } > 0)
            .then(|| MessageNodeDirect::Update(self).to_intern_out(dst))
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use flarch::data_storage::DataStorageTemp;

    use crate::testing::flo::Wallet;

    use super::*;

    use std::error::Error;

    #[test]
    fn test_choice() -> Result<(), Box<dyn Error>> {
        let our_id = NodeID::rnd();
        let mut dht = Messages::new(
            Box::new(DataStorageTemp::new()),
            DHTConfig::default(),
            our_id,
        )
        .0;

        let mut wallet = Wallet::new();
        let realm = wallet.get_realm();
        dht.msg_in_storage(DHTStorageIn::StoreFlo(realm.flo().clone()));
        let out = dht.msg_in_storage(DHTStorageIn::ReadFlo(realm.global_id()));
        assert_eq!(1, out.len());
        assert_eq!(
            *out.get(0).unwrap(),
            InternOut::Storage(DHTStorageOut::FloValue((realm.flo().clone(), vec![])))
        );
        Ok(())
    }
}
