use std::collections::HashMap;

use flarch::{
    broker::SubsystemHandler,
    data_storage::DataStorage,
    nodeids::{NodeID, U256},
    platform_async_trait,
};
use flcrypto::tofrombytes::ToFromBytes;
use metrics::increment_counter;
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
    broker::{DHTStorageIn, DHTStorageOut, MODULE_NAME},
    core::*,
};

/// The messages here represent all possible interactions with this module.
#[derive(Debug, Clone, PartialEq)]
pub enum InternIn {
    Routing(DHTRouterOut),
    Storage(DHTStorageIn),
    /// Ask all neighbors to sync with us.
    PropagateFlos,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InternOut {
    Routing(DHTRouterIn),
    Storage(DHTStorageOut),
}

/// These messages are sent to the closest node given in the DHTRouting message.
/// Per default, the 'key' value of the DHTRouting will be filled with the FloID.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageClosest {
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
pub enum MessageDest {
    // Returns the result of the requested Flo, including any available Cuckoo-IDs.
    FloValue(FloCuckoo),
    // Indicates this Flo is not in the closest node.
    UnknownFlo(GlobalID),
    // The Cuckoo-IDs stored next to the Flo composed of the GlobalID
    CookooIDs(GlobalID, Vec<FloID>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageNeighbour {
    RequestRealmIDs,
    AvailableRealmIDs(Vec<RealmID>),
    RequestFloMetas(RealmID),
    AvailableFlos(RealmID, Vec<FloMeta>),
    RequestFlos(RealmID, Vec<FloID>),
    Flos(Vec<FloCuckoo>),
}

#[derive(Debug, Clone)]
pub struct RealmStats {
    pub real_size: usize,
    pub size: u64,
    pub flos: usize,
    pub distribution: Vec<usize>,
    pub config: RealmConfig,
}

#[derive(Debug, Default, Clone)]
pub struct Stats {
    pub realm_stats: HashMap<RealmID, RealmStats>,
    pub system_realms: Vec<RealmID>,
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

pub static mut EVIL_NO_FORWARD: bool = false;

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
        let mut msgs = Self {
            realms,
            config,
            our_id: our_node,
            nodes: vec![],
            ds,
            tx: Some(tx),
        };
        msgs.store();
        (msgs, rx)
    }

    fn msg_dht_router(&mut self, msg: DHTRouterOut) -> Vec<InternOut> {
        match msg {
            DHTRouterOut::MessageRouting(origin, _last_hop, _next_hop, key, msg) => msg
                .unwrap_yaml(MODULE_NAME)
                .map(|mn| self.msg_routing(false, origin, key, mn)),
            DHTRouterOut::MessageClosest(origin, _last_hop, key, msg) => msg
                .unwrap_yaml(MODULE_NAME)
                .map(|mn| self.msg_routing(true, origin, key, mn)),
            DHTRouterOut::MessageDest(_origin, _last_hop, msg) => {
                msg.unwrap_yaml(MODULE_NAME).map(|mn| self.msg_dest(mn))
            }
            DHTRouterOut::NodeList(nodes) => {
                self.nodes = nodes;
                None
            }
            DHTRouterOut::MessageNeighbour(origin, msg) => msg
                .unwrap_yaml(MODULE_NAME)
                .map(|mn| self.msg_neighbour(origin, mn)),
            DHTRouterOut::SystemRealm(realm_id) => {
                if !self.config.realms.contains(&realm_id) {
                    self.config.realms.push(realm_id);
                }
                None
            }
        }
        .unwrap_or(vec![])
    }

    fn msg_dht_storage(&mut self, msg: DHTStorageIn) -> Vec<InternOut> {
        // log::warn!("Storing {msg:?}");
        match msg {
            DHTStorageIn::StoreFlo(flo) => self.store_flo(flo),
            DHTStorageIn::ReadFlo(id) => vec![match self.read_flo(&id) {
                Some(df) => DHTStorageOut::FloValue(df.clone()).into(),
                None => MessageClosest::ReadFlo(id.realm_id().clone())
                    .to_intern_out(id.flo_id().clone().into())
                    // .inspect(|msg| log::info!("{} sends {msg:?}", self.our_id))
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
                    MessageClosest::GetCuckooIDs(id.realm_id().clone())
                        .to_intern_out(id.flo_id().clone().into())
                        .expect("Creating GetCuckoos"),
                );
                out
            }
            DHTStorageIn::SyncFromNeighbors => vec![MessageNeighbour::RequestRealmIDs
                .to_broadcast()
                .expect("Creating Request broadcast")],
            DHTStorageIn::GetRealms => {
                vec![DHTStorageOut::RealmIDs(self.realms.keys().cloned().collect()).into()]
            }
            DHTStorageIn::GetFlos => {
                vec![DHTStorageOut::FloValues(
                    self.realms
                        .iter()
                        .flat_map(|realm| realm.1.get_all_flo_cuckoos())
                        .collect::<Vec<_>>(),
                )
                .into()]
            }
        }
    }

    fn msg_routing(
        &mut self,
        _closest: bool,
        origin: NodeID,
        key: U256,
        msg: MessageClosest,
    ) -> Vec<InternOut> {
        let fid: FloID = key.into();
        match msg {
            MessageClosest::StoreFlo(flo) => {
                return self.store_flo(flo);
            }
            MessageClosest::ReadFlo(rid) => {
                // log::info!(
                //     "{} got request for {}/{} from {}",
                //     self.our_id,
                //     key,
                //     rid,
                //     origin
                // );
                // log::info!("{}", self.realms.get(&rid).is_some());
                if let Some(fc) = self
                    .realms
                    .get(&rid)
                    .and_then(|realm| realm.get_flo_cuckoo(&fid))
                {
                    // log::info!("sends flo {:?}", fc.0);
                    return MessageDest::FloValue(fc)
                        .to_intern_out(origin)
                        // .inspect(|msg| log::info!("{} sends {msg:?}", self.our_id))
                        .map_or(vec![], |msg| vec![msg]);
                }
            }
            MessageClosest::GetCuckooIDs(rid) => {
                let parent = GlobalID::new(rid.clone(), fid.clone());
                return self
                    .realms
                    .get(&rid)
                    .and_then(|realm| realm.get_cuckoo_ids(&fid))
                    .map_or(vec![], |ids| {
                        MessageDest::CookooIDs(parent, ids)
                            .to_intern_out(origin)
                            .map_or(vec![], |msg| vec![msg])
                    });
            }
            MessageClosest::StoreCuckooID(gid) => {
                self.realms
                    .get_mut(&gid.realm_id())
                    .map(|realm| realm.store_cuckoo_id(&fid, gid.flo_id().clone()));
            }
        }
        vec![]
    }

    fn msg_dest(&mut self, msg: MessageDest) -> Vec<InternOut> {
        match msg {
            MessageDest::FloValue(fc) => {
                // log::info!("{} stores {:?}", self.our_id, flo.0);
                self.store_flo(fc.0.clone());
                self.realms
                    .get_mut(&fc.0.realm_id())
                    .map(|realm| realm.store_cuckoo_ids(&fc.0.flo_id(), fc.1.clone()));
                Some(DHTStorageOut::FloValue(fc).into())
            }
            MessageDest::UnknownFlo(key) => Some(DHTStorageOut::ValueMissing(key).into()),
            MessageDest::CookooIDs(gid, cids) => Some(DHTStorageOut::CuckooIDs(gid, cids).into()),
        }
        .map_or(vec![], |msg| vec![msg])
    }

    fn msg_neighbour(&mut self, origin: NodeID, msg: MessageNeighbour) -> Vec<InternOut> {
        // log::trace!("{} syncs {:?}", self.our_id, msg);
        // if let Sync::RequestFloMetas(rid) = &msg {
        //     log::trace!(
        //         "{} receives ReqFloMet from {} and will reply: {}",
        //         self.our_id,
        //         origin,
        //         self.realms
        //             .get(rid)
        //             .map(|realm| realm
        //                 .get_flo_metas()
        //                 .iter()
        //                 .map(|fm| format!("{}", fm.id))
        //                 .sorted()
        //                 .collect::<Vec<_>>()
        //                 .join(" - "))
        //             .unwrap_or("".to_string())
        //     );
        // }
        match msg {
            MessageNeighbour::RequestRealmIDs => vec![MessageNeighbour::AvailableRealmIDs(
                self.realms.keys().cloned().collect(),
            )],
            MessageNeighbour::AvailableRealmIDs(realm_ids) => {
                let accepted_realms = realm_ids
                    .into_iter()
                    .filter(|rid| self.config.accepts_realm(&rid))
                    .collect::<Vec<_>>();
                accepted_realms
                    .iter()
                    .filter(|id| !self.realms.contains_key(id))
                    .map(|rid| MessageNeighbour::RequestFlos(rid.clone(), vec![(**rid).into()]))
                    .chain(
                        accepted_realms
                            .iter()
                            .map(|rid| MessageNeighbour::RequestFloMetas(rid.clone())),
                    )
                    .collect()
            }
            MessageNeighbour::AvailableFlos(realm_id, flo_metas) => self
                .realms
                .get(&realm_id)
                .and_then(|realm| realm.sync_available(&flo_metas))
                .map_or(vec![], |needed| {
                    vec![MessageNeighbour::RequestFlos(realm_id, needed)]
                }),
            MessageNeighbour::RequestFloMetas(realm_id) => {
                if unsafe { !EVIL_NO_FORWARD } {
                    increment_counter!("fledger_forwarded_flo_meta_requests_total");
                    self.realms
                        .get(&realm_id)
                        .map(|realm| realm.get_flo_metas())
                        .map_or(vec![], |fm| {
                            vec![MessageNeighbour::AvailableFlos(realm_id, fm)]
                        })
                } else {
                    increment_counter!("fledger_blocked_flo_meta_requests_total");
                    vec![]
                }
            }
            MessageNeighbour::RequestFlos(realm_id, flo_ids) => {
                if unsafe { !EVIL_NO_FORWARD } {
                    increment_counter!("fledger_forwarded_flo_requests_total");
                    self.realms
                        .get(&realm_id)
                        .map(|realm| {
                            flo_ids
                                .iter()
                                .filter_map(|id| realm.get_flo_cuckoo(id))
                                .collect::<Vec<_>>()
                        })
                        .map_or(vec![], |flos| vec![MessageNeighbour::Flos(flos)])
                } else {
                    increment_counter!("fledger_blocked_flo_requests_total");
                    vec![]
                }
            }
            MessageNeighbour::Flos(flo_cuckoos) => {
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
        .filter_map(|msg| msg.to_neighbour(origin))
        .collect()
    }

    fn read_flo(&self, id: &GlobalID) -> Option<FloCuckoo> {
        self.realms
            .get(id.realm_id())
            .and_then(|realm| realm.get_flo_cuckoo(id.flo_id()))
    }

    fn store_flo(&mut self, flo: Flo) -> Vec<InternOut> {
        increment_counter!("fledger_ds_store_flo_total");
        let mut res = vec![];
        if self.upsert_flo(flo.clone()) {
            // log::info!("{}: store_flo", self.our_id);
            // TODO: this should not be sent in all cases...
            res.extend(vec![MessageClosest::StoreFlo(flo.clone())
                .to_intern_out(flo.flo_id().into())
                .expect("Storing new DHT")]);
        }
        if let Some(parent) = flo.flo_config().cuckoo_parent() {
            res.extend(vec![MessageClosest::StoreCuckooID(flo.global_id())
                .to_intern_out(*parent.clone())
                .expect("Storing new Cuckoo")])
        }
        res
    }

    // Try really hard to store the flo.
    // Either its realm is already known, or it is a new realm.
    // When 'true' is returned, then the flo has been stored.
    fn upsert_flo(&mut self, flo: Flo) -> bool {
        log::info!(
            "{} store_flo {}({}/{}) {}",
            self.our_id,
            flo.flo_type(),
            flo.flo_id(),
            flo.realm_id(),
            flo.version()
        );
        // log::info!(
        //     "{} has realm: {}",
        //     self.our_id,
        //     self.realms.get(&flo.realm_id()).is_some()
        // );
        // log::info!(
        //     "Can create FloRealm of {}/{:?}: {:?}",
        //     flo.flo_type(),
        //     flo.data(),
        //     TryInto::<FloRealm>::try_into(flo.clone())
        // );
        let modification = self
            .realms
            .get_mut(&flo.realm_id())
            // .inspect(|rs| log::info!("{} has realm", self.our_id))
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

    fn create_realm(&mut self, realm: FloRealm) -> anyhow::Result<()> {
        if !self.config.accepts_realm(&realm.realm_id()) {
            return Err(CoreError::RealmNotAccepted.into());
        }
        // log::debug!(
        //     "{} creating realm {}/{}",
        //     self.our_id,
        //     realm.realm_id(),
        //     realm.version()
        // );
        let id = realm.flo().realm_id();
        let dsc = RealmStorage::new(self.config.clone(), self.our_id, realm)?;
        self.realms.insert(id.clone(), dsc);
        // log::info!("Realm {}: {}", id, self.realms.get(&id).is_some());
        Ok(())
    }

    fn store(&mut self) {
        self.tx.clone().map(|tx| {
            tx.send(Stats::from_realms(&self.realms, self.config.realms.clone()))
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
                InternIn::Routing(dhtrouting_out) => self.msg_dht_router(dhtrouting_out),
                InternIn::Storage(dhtstorage_in) => self.msg_dht_storage(dhtstorage_in),
                InternIn::PropagateFlos => {
                    MessageNeighbour::AvailableRealmIDs(self.realms.keys().cloned().collect())
                        .to_broadcast()
                        .map_or(vec![], |msg| vec![msg])
                }
            })
            //.inspect(|msg| log::debug!("{_id}: Out: {msg:?}"))
            .collect()
    }
}

impl Stats {
    fn from_realms(realms: &HashMap<RealmID, RealmStorage>, system_realms: Vec<RealmID>) -> Self {
        Self {
            realm_stats: realms
                .iter()
                .map(|(id, realm)| (id.clone(), RealmStats::from_realm(realm)))
                .collect(),
            system_realms,
        }
    }
}

impl RealmStats {
    fn from_realm(realm: &RealmStorage) -> Self {
        Self {
            real_size: realm.size(),
            size: realm.size,
            flos: realm.flo_count(),
            distribution: realm.flo_distribution(),
            config: realm.realm_config().clone(),
        }
    }
}

impl MessageClosest {
    fn to_intern_out(&self, dst: NodeID) -> Option<InternOut> {
        NetworkWrapper::wrap_yaml(MODULE_NAME, self)
            .ok()
            .map(|msg_wrap| InternOut::Routing(DHTRouterIn::MessageClosest(dst, msg_wrap)))
    }
}

impl MessageDest {
    fn to_intern_out(&self, dst: NodeID) -> Option<InternOut> {
        NetworkWrapper::wrap_yaml(MODULE_NAME, self)
            .ok()
            .map(|msg_wrap| InternOut::Routing(DHTRouterIn::MessageDirect(dst, msg_wrap)))
    }
}

impl From<DHTStorageOut> for InternOut {
    fn from(value: DHTStorageOut) -> Self {
        InternOut::Storage(value)
    }
}

impl MessageNeighbour {
    fn to_neighbour(self, dst: NodeID) -> Option<InternOut> {
        (match &self {
            MessageNeighbour::AvailableRealmIDs(realm_ids) => realm_ids.len(),
            MessageNeighbour::AvailableFlos(_, flo_metas) => flo_metas.len(),
            MessageNeighbour::RequestFlos(_, flo_ids) => flo_ids.len(),
            MessageNeighbour::Flos(items) => items.len(),
            _ => 1,
        } > 0)
            .then(|| {
                NetworkWrapper::wrap_yaml(MODULE_NAME, &self)
                    .ok()
                    .map(|msg_wrap| {
                        InternOut::Routing(DHTRouterIn::MessageNeighbour(dst, msg_wrap))
                    })
            })
            .flatten()
    }

    fn to_broadcast(self) -> Option<InternOut> {
        (match &self {
            MessageNeighbour::AvailableRealmIDs(realm_ids) => realm_ids.len(),
            MessageNeighbour::AvailableFlos(_, flo_metas) => flo_metas.len(),
            MessageNeighbour::RequestFlos(_, flo_ids) => flo_ids.len(),
            MessageNeighbour::Flos(items) => items.len(),
            _ => 1,
        } > 0)
            .then(|| {
                NetworkWrapper::wrap_yaml(MODULE_NAME, &self)
                    .ok()
                    .map(|msg_wrap| InternOut::Routing(DHTRouterIn::MessageBroadcast(msg_wrap)))
            })
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use flarch::{data_storage::DataStorageTemp, start_logging};
    use flcrypto::{access::Condition, signer_ed25519::SignerEd25519};

    use crate::{
        flo::realm::Realm,
        testing::wallet::{FloTesting, Wallet},
    };

    use super::*;

    #[test]
    fn test_choice() -> anyhow::Result<()> {
        let our_id = NodeID::rnd();
        let mut dht = Messages::new(
            Box::new(DataStorageTemp::new()),
            DHTConfig::default(),
            our_id,
        )
        .0;

        let mut wallet = Wallet::new();
        let realm = wallet.get_realm();
        dht.msg_dht_storage(DHTStorageIn::StoreFlo(realm.flo().clone()));
        let out = dht.msg_dht_storage(DHTStorageIn::ReadFlo(realm.global_id()));
        assert_eq!(1, out.len());
        assert_eq!(
            *out.get(0).unwrap(),
            InternOut::Storage(DHTStorageOut::FloValue((realm.flo().clone(), vec![])))
        );
        Ok(())
    }

    #[test]
    fn serialize() -> anyhow::Result<()> {
        let fr = FloRealm::from_type(
            RealmID::rnd(),
            Condition::Pass,
            Realm::new(
                "root".into(),
                RealmConfig {
                    max_space: 1000,
                    max_flo_size: 1000,
                },
            ),
            &[],
        )?;

        let out = NetworkWrapper::wrap_yaml(
            MODULE_NAME,
            &MessageDest::FloValue((fr.flo().clone(), vec![])),
        )
        .unwrap();

        if let MessageDest::FloValue(flo) = out.unwrap_yaml(MODULE_NAME).unwrap() {
            let fr2 = TryInto::<FloRealm>::try_into(flo.0)?;
            assert_eq!(fr, fr2);
        } else {
            return Err(anyhow::anyhow!("Didn't find message"));
        }
        Ok(())
    }

    // Verify that a cuckoo page is correctly stored and retrievable as a cuckoo page.
    #[test]
    fn store_cuckoos() -> anyhow::Result<()> {
        start_logging();
        let mut msg = Messages::new(
            DataStorageTemp::new_box(),
            DHTConfig::default(),
            NodeID::rnd(),
        )
        .0;
        let fr = FloRealm::from_type(
            RealmID::rnd(),
            Condition::Pass,
            Realm::new(
                "root".into(),
                RealmConfig {
                    max_space: 10000,
                    max_flo_size: 1000,
                },
            ),
            &[],
        )?;
        msg.msg_dht_storage(DHTStorageIn::StoreFlo(fr.clone().into()));

        let signer = SignerEd25519::new();
        let fb_root =
            FloTesting::new_cuckoo(fr.realm_id(), "data", Cuckoo::Duration(100000), &signer);
        msg.msg_dht_storage(DHTStorageIn::StoreFlo(fb_root.clone().into()));
        let fb_cu = FloTesting::new_cuckoo(
            fr.realm_id(),
            "data2",
            Cuckoo::Parent(fb_root.flo_id()),
            &signer,
        );
        let ans = msg.msg_dht_storage(DHTStorageIn::StoreFlo(fb_cu.clone().into()));
        log::info!("{ans:?}");

        let ans = msg.msg_dht_storage(DHTStorageIn::ReadCuckooIDs(fb_root.global_id()));
        if let Some(InternOut::Storage(DHTStorageOut::CuckooIDs(_, cs))) = ans.get(0) {
            assert_eq!(Some(&fb_cu.flo_id()), cs.get(0));
        } else {
            assert!(false, "Answer should be CuckooIDs");
        }

        Ok(())
    }
}
