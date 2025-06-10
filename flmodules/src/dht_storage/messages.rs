use std::{any::type_name, collections::HashMap};

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

#[derive(Serialize, Debug, Default, Clone)]
pub struct DsMetrics {
    pub store_flo_total: u32,
    pub flo_value_sent_total: u32,
    pub flo_value_sent_blocked_total: u32,
    pub available_flos_sent_total: u32,
    pub available_flos_sent_blocked_total: u32,
    pub request_flos_sent_total: u32,
    pub flos_sent_total: u32,
    pub flos_sent_blocked_total: u32,
    pub max_flo_metas_received_in_available_flos: u32,
    pub max_flo_metas_requested_in_request_flos: u32,
    pub max_flo_ids_received_in_request_flos: u32,
    pub max_flos_sent_in_flos: u32,
    pub max_flos_received_in_flos: u32,
}

/**
 * It is necessary to call `self.refresh_stats()` after each change of a stat,
 *   and using methods would surely be worse than using macros, unless
 *   we would use a method for each stat.
 *
 *   Since we want it to be as easy as possible to add new stats, we use macros.
 */
macro_rules! increment_stat {
    ($self:ident, $stat:expr) => {
        $stat += 1;
        $self.refresh_stats();
    };
}

// macro_rules! absolute_stat {
//     ($self:ident, $stat:expr, $value:expr) => {
//         $stat = $value;
//         $self.refresh_stats();
//     };
// }

macro_rules! max_stat {
    ($self:ident, $stat:expr, $value:expr) => {
        if $value > $stat {
            $stat = $value;
            $self.refresh_stats();
        }
    };
}

#[derive(Debug, Default, Clone)]
pub struct Stats {
    pub realm_stats: HashMap<RealmID, RealmStats>,
    pub system_realms: Vec<RealmID>,
    pub experiment_stats: DsMetrics,
}

/// The message handling part, but only for DHTStorage messages.
pub struct Messages {
    realms: HashMap<RealmID, RealmStorage>,
    nodes: Vec<NodeID>,
    config: DHTConfig,
    our_id: NodeID,
    ds: Box<dyn DataStorage + Send>,
    tx: Option<watch::Sender<Stats>>,

    experiment_stats: DsMetrics,
}

pub static mut EVIL_NO_FORWARD: bool = false;

impl Messages {
    /// Returns a new simulation_chat module.
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

            experiment_stats: DsMetrics::new(),
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
                    if unsafe { !EVIL_NO_FORWARD } {
                        increment_stat!(self, self.experiment_stats.flo_value_sent_total);
                        return MessageDest::FloValue(fc)
                            .to_intern_out(origin)
                            // .inspect(|msg| log::info!("{} sends {msg:?}", self.our_id))
                            .map_or(vec![], |msg| vec![msg]);
                    } else {
                        increment_stat!(self, self.experiment_stats.flo_value_sent_blocked_total);
                        return vec![];
                    }
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
            MessageNeighbour::RequestFloMetas(realm_id) => {
                if unsafe { !EVIL_NO_FORWARD } {
                    increment_stat!(self, self.experiment_stats.available_flos_sent_total);
                    self.realms
                        .get(&realm_id)
                        .map(|realm| realm.get_flo_metas())
                        .map_or(vec![], |fm| {
                            vec![MessageNeighbour::AvailableFlos(realm_id, fm)]
                        })
                } else {
                    increment_stat!(
                        self,
                        self.experiment_stats.available_flos_sent_blocked_total
                    );
                    vec![]
                }
            }
            MessageNeighbour::AvailableFlos(realm_id, flo_metas) => {
                max_stat!(
                    self,
                    self.experiment_stats
                        .max_flo_metas_received_in_available_flos,
                    flo_metas.len() as u32
                );

                self.realms
                    .get(&realm_id)
                    .and_then(|realm| realm.sync_available(&flo_metas))
                    .map_or(vec![], |needed| {
                        max_stat!(
                            self,
                            self.experiment_stats
                                .max_flo_metas_requested_in_request_flos,
                            needed.len() as u32
                        );
                        vec![MessageNeighbour::RequestFlos(realm_id, needed)]
                    })
            }
            MessageNeighbour::RequestFlos(realm_id, flo_ids) => {
                max_stat!(
                    self,
                    self.experiment_stats.max_flo_ids_received_in_request_flos,
                    flo_ids.len() as u32
                );
                if unsafe { !EVIL_NO_FORWARD } {
                    increment_stat!(self, self.experiment_stats.flos_sent_total);
                    self.realms
                        .get(&realm_id)
                        .map(|realm| {
                            let flos = flo_ids
                                .iter()
                                .filter_map(|id| realm.get_flo_cuckoo(id))
                                .collect::<Vec<_>>();

                            let realm = flos
                                .iter()
                                .find(|flo| flo.0.flo_type() == type_name::<FloRealm>());

                            let flo_to_forward = realm.or(flos.last());
                            if let Some(flo) = flo_to_forward {
                                return vec![flo.clone()];
                            } else {
                                return vec![];
                            }
                        })
                        .map_or(vec![], |flos| {
                            //log::info!("Flos to send: {}", flos.len());
                            max_stat!(
                                self,
                                self.experiment_stats.max_flos_sent_in_flos,
                                flos.len() as u32
                            );
                            vec![MessageNeighbour::Flos(flos)]
                        })
                } else {
                    increment_stat!(self, self.experiment_stats.flos_sent_blocked_total);
                    vec![]
                }
            }
            MessageNeighbour::Flos(flo_cuckoos) => {
                max_stat!(
                    self,
                    self.experiment_stats.max_flos_received_in_flos,
                    flo_cuckoos.len() as u32
                );
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
        increment_stat!(self, self.experiment_stats.store_flo_total);
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
        // log::info!(
        //     "{} store_flo {}({}/{}) {}",
        //     self.our_id,
        //     flo.flo_type(),
        //     flo.flo_id(),
        //     flo.realm_id(),
        //     flo.version()
        // );
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
        self.refresh_stats();
        serde_yaml::to_string(&self.realms)
            .ok()
            .map(|s| (*self.ds).set(MODULE_NAME, &s));
    }

    fn refresh_stats(&mut self) {
        self.tx.clone().map(|tx| {
            tx.send(Stats::from_realms(
                &self.realms,
                self.config.realms.clone(),
                self.experiment_stats.clone(),
            ))
            .is_err()
            .then(|| self.tx = None)
        });
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

impl DsMetrics {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Stats {
    fn from_realms(
        realms: &HashMap<RealmID, RealmStorage>,
        system_realms: Vec<RealmID>,
        experiment_stats: DsMetrics,
    ) -> Self {
        Self {
            realm_stats: realms
                .iter()
                .map(|(id, realm)| (id.clone(), RealmStats::from_realm(realm)))
                .collect(),
            system_realms,
            experiment_stats,
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
