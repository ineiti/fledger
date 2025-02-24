use std::{collections::HashMap, time::Duration};

use async_recursion::async_recursion;
use flarch::{
    broker::{Broker, BrokerError},
    data_storage::DataStorage,
};
use flcrypto::{
    access::{Badge, BadgeID, BadgeSignature, CalcSignature, Condition, Version},
    signer::{Signature, SignerError, Verifier, KeyPairID},
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;
use tokio::{sync::watch, time::timeout};

use crate::{
    dht_router::broker::BrokerDHTRouter,
    flo::{
        crypto::{BadgeCond, Rules, ACE},
        flo::{Flo, FloError, FloID, FloWrapper, UpdateSign},
        realm::{GlobalID, RealmID},
    },
    timer::Timer,
};
use flarch::nodeids::NodeID;

use super::{
    core::{CoreError, DHTConfig, FloCuckoo},
    messages::{InternIn, InternOut, Messages, Stats},
};

pub(super) const MODULE_NAME: &str = "DHTStorage";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DHTStorageIn {
    StoreFlo(Flo),
    ReadFlo(GlobalID),
    ReadCuckooIDs(GlobalID),
    GetRealms,
    /// Ask all neighbors for their realms and Flos
    SyncNeighbors,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DHTStorageOut {
    FloValue(FloCuckoo),
    RealmIDs(Vec<RealmID>),
    CuckooIDs(GlobalID, Vec<FloID>),
    ValueMissing(GlobalID),
}

/// This links the DHTStorage module with other modules, so that
/// all messages are correctly translated from one to the other.
/// For this example, it uses the RandomConnections module to communicate
/// with other nodes.
///
/// The [DHTStorage] holds the [Translate] and offers convenience methods
/// to interact with [Translate] and [DHTStorageMessage].
#[derive(Clone)]
pub struct DHTStorage {
    /// Represents the underlying broker.
    pub broker: Broker<DHTStorageIn, DHTStorageOut>,
    pub stats: watch::Receiver<Stats>,
    intern: Broker<InternIn, InternOut>,
    config: DHTConfig,
    _our_id: NodeID,
}

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("CoreError({0})")]
    CoreError(#[from] CoreError),
    #[error("FloError({0})")]
    FloError(#[from] FloError),
    #[error("BrokerError({0})")]
    BrokerError(#[from] BrokerError),
    #[error("SignerError({0})")]
    SignerError(#[from] SignerError),
    #[error("FloWrapper.rules has no update rule")]
    UpdateRuleMissing,
    #[error("Couldn't find a Flo with this ID")]
    FloNotFound,
    #[error("Timeout while waiting for response")]
    TimeoutError,
    #[error("Coudn't serialize: {0}")]
    SerdeError(#[from] serde_yaml::Error),
}

impl DHTStorage {
    pub async fn start(
        ds: Box<dyn DataStorage + Send>,
        our_id: NodeID,
        config: DHTConfig,
        dht_router: BrokerDHTRouter,
        timer: &mut Timer,
    ) -> Result<Self, StorageError> {
        let (messages, stats) = Messages::new(ds, config.clone(), our_id.clone());
        let mut intern = Broker::new();
        intern.add_handler(Box::new(messages)).await?;

        intern
            .add_translator_link(
                dht_router,
                Box::new(|msg| match msg {
                    InternOut::Routing(dhtrouting_in) => Some(dhtrouting_in),
                    _ => None,
                }),
                Box::new(|msg| Some(InternIn::Routing(msg))),
            )
            .await?;

        let broker = Broker::new();
        intern
            .add_translator_direct(
                broker.clone(),
                Box::new(|msg| Some(InternIn::Storage(msg))),
                Box::new(|msg| match msg {
                    InternOut::Storage(dhtstorage_out) => Some(dhtstorage_out),
                    _ => None,
                }),
            )
            .await?;
        timer
            .tick_minute(
                intern.clone(),
                InternIn::Storage(DHTStorageIn::SyncNeighbors),
            )
            .await?;

        Ok(DHTStorage {
            broker,
            stats,
            intern,
            config,
            _our_id: our_id,
        })
    }

    pub fn store_flo(&mut self, flo: Flo) -> Result<(), BrokerError> {
        self.broker.emit_msg_in(DHTStorageIn::StoreFlo(flo))
    }

    pub fn propagate(&mut self) -> Result<(), StorageError> {
        Ok(self.intern.emit_msg_in(InternIn::BroadcastSync)?)
    }

    pub async fn get_realm_ids(&mut self) -> Result<Vec<RealmID>, StorageError> {
        self.send_wait(DHTStorageIn::GetRealms, &|msg| match msg {
            DHTStorageOut::RealmIDs(realms) => Some(realms),
            _ => None,
        })
        .await
    }

    pub async fn get_flo<T: Serialize + DeserializeOwned + Clone>(
        &mut self,
        id: &GlobalID,
    ) -> Result<FloWrapper<T>, StorageError> {
        Ok(self
            .send_wait(DHTStorageIn::ReadFlo(id.clone()), &|msg| match msg {
                DHTStorageOut::FloValue((flo, _)) => (flo.global_id() == *id).then_some(flo),
                _ => None,
            })
            .await?
            .try_into()?)
    }

    pub async fn get_cuckoos(&mut self, id: &GlobalID) -> Result<Vec<FloID>, StorageError> {
        Ok(self
            .send_wait(DHTStorageIn::ReadCuckooIDs(id.clone()), &|msg| match msg {
                DHTStorageOut::CuckooIDs(rid, ids) => (&rid == id).then_some(ids),
                _ => None,
            })
            .await?)
    }

    pub fn sync(&mut self) -> Result<(), BrokerError> {
        self.broker.emit_msg_in(DHTStorageIn::SyncNeighbors)
    }

    pub async fn get_badge_signature<T: Serialize + DeserializeOwned + Clone>(
        &mut self,
        fw: &FloWrapper<T>,
        update: &T,
    ) -> Result<BadgeSignature, StorageError> {
        let cond = match fw.rules() {
            Rules::ACE(version) => self
                .get_flo::<ACE>(&GlobalID::new(fw.realm_id(), (*version.get_id()).into()))
                .await?
                .cache()
                .update()
                .ok_or(StorageError::UpdateRuleMissing),
            Rules::Update(condition) => Ok(condition.clone()),
            Rules::None => Err(StorageError::UpdateRuleMissing),
        }?;
        let msg = fw.hash_update(update).bytes();
        let badges = self
            .recurse_condition(&fw.realm_id(), BadgeSig::default(), &cond)
            .await?;
        let cs =
            CalcSignature::from_cond_badges(&cond, badges.badges, badges.signatures, msg.clone())?;
        Ok(BadgeSignature::from_calc_signature(cs, cond, msg))
    }

    pub async fn get_update_sign<T: Serialize + DeserializeOwned + Clone>(
        &mut self,
        fw: &FloWrapper<T>,
        update: T,
        rules: Option<Rules>,
    ) -> Result<UpdateSign<T>, StorageError> {
        let bs = self.get_badge_signature(fw, &update).await?;
        Ok(UpdateSign::new(
            update,
            rules.unwrap_or_else(|| fw.rules().clone()),
            bs,
        ))
    }

    async fn send_wait<T>(
        &mut self,
        msg_in: DHTStorageIn,
        check: &(dyn Fn(DHTStorageOut) -> Option<T> + Sync),
    ) -> Result<T, StorageError> {
        // using the internal broker directly to avoid one conversion.
        let dur = Duration::from_millis(self.config.timeout);
        let (mut tap, tap_id) = self.intern.get_tap_out().await?;
        self.intern.emit_msg_in(InternIn::Storage(msg_in))?;
        let res = timeout(dur, async move {
            while let Some(msg_int) = tap.recv().await {
                if let InternOut::Storage(msg_out) = msg_int {
                    if let Some(res) = check(msg_out) {
                        return Ok(res);
                    }
                }
            }
            Err(BrokerError::Translation)
        })
        .await
        .map_err(|_| StorageError::TimeoutError);
        // Need to remove subsystem before checking error.
        self.intern.remove_subsystem(tap_id).await?;
        Ok(res??)
    }

    #[async_recursion(?Send)]
    async fn recurse_condition(
        &mut self,
        realm_id: &RealmID,
        mut bad_sig: BadgeSig,
        condition: &Condition,
    ) -> Result<BadgeSig, StorageError> {
        match condition {
            Condition::Verifier(vid) => {
                bad_sig.signatures.insert(vid.clone(), None);
            }
            Condition::Badge(version) => {
                if bad_sig.badges.contains_key(version) {
                    log::warn!("Loop in condition");
                } else {
                    let cond = self
                        .get_flo::<BadgeCond>(&GlobalID::new(
                            realm_id.clone(),
                            (*version.get_id()).into(),
                        ))
                        .await?;
                    bad_sig
                        .badges
                        .insert(version.clone(), cond.badge_cond().clone());
                }
            }
            Condition::NofT(_, vec) => {
                for v in vec {
                    bad_sig = self.recurse_condition(realm_id, bad_sig, v).await?;
                }
            }
            Condition::Pass => (),
        }
        Ok(bad_sig)
    }
}

#[derive(Default, Debug)]
struct BadgeSig {
    badges: HashMap<Version<BadgeID>, Condition>,
    signatures: HashMap<KeyPairID, Option<(Verifier, Signature)>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    use crate::{
        dht_router::{broker::DHTRouter, kademlia::Config},
        dht_storage::core::{Cuckoo, RealmConfig},
        flo::realm::{FloRealm, Realm},
        testing::{
            flo::{FloTesting, Wallet},
            network_simul::NetworkSimul,
        },
        timer::{Timer, TimerMessage},
    };
    use flarch::{data_storage::DataStorageTemp, start_logging_filter_level};

    struct SimulStorage {
        simul: NetworkSimul,
        tick: Timer,
        nodes: Vec<DHTStorage>,
        realm: FloRealm,
        _wallet: Wallet,
    }

    impl SimulStorage {
        async fn new() -> Result<Self, Box<dyn Error>> {
            let mut wallet = Wallet::new();
            Ok(Self {
                simul: NetworkSimul::new().await?,
                tick: Timer::simul(),
                nodes: vec![],
                realm: FloRealm::new(
                    "root",
                    wallet.get_badge_rules(),
                    RealmConfig {
                        max_space: 1e6 as u64,
                        max_flo_size: 1e4 as u32,
                    },
                )?,
                _wallet: wallet,
            })
        }

        async fn node(&mut self) -> Result<DHTStorage, Box<dyn Error>> {
            let n = self.simul.new_node().await?;
            let id = n.config.info.get_id();
            let dht_router =
                DHTRouter::start(id, n.router, &mut self.tick, Config::default()).await?;
            let n_s = DHTStorage::start(
                Box::new(DataStorageTemp::new()),
                n.config.info.get_id(),
                DHTConfig::default(),
                dht_router.broker.clone(),
                &mut self.tick,
            )
            .await?;
            self.nodes.push(n_s.clone());
            self.simul.send_node_info().await?;
            self.tick.broker.emit_msg_out(TimerMessage::Second)?;
            Ok(n_s)
        }

        async fn settle_all(&mut self) {
            for n in &mut self.nodes {
                n.broker.settle(vec![]).await.expect("Settling node broker");
            }
            self.simul.settle().await.expect("Settling simul broker");
        }

        async fn sync_all(&mut self) -> Result<(), Box<dyn Error>> {
            for n in &mut self.nodes {
                n.sync()?;
            }
            self.settle_all().await;
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_storage_propagation() -> Result<(), Box<dyn Error>> {
        start_logging_filter_level(vec!["flmodules"], log::LevelFilter::Info);

        let mut router = SimulStorage::new().await?;
        let mut ds_0 = router.node().await?;
        let mut ds_1 = router.node().await?;
        router.simul.send_node_info().await?;

        // Testing copy of flo in other node(s) when they are present during
        // the storage operation.
        ds_0.store_flo(router.realm.clone().into())?;
        router.sync_all().await?;
        assert!(ds_0
            .get_flo::<Realm>(&router.realm.global_id())
            .await
            .is_ok());
        assert!(ds_1
            .get_flo::<Realm>(&router.realm.global_id())
            .await
            .is_ok());

        // Testing copy of flo in other node(s) who join later.
        log::info!("Copying realm to new nodes");
        let mut ds_2 = router.node().await?;

        router.simul.send_node_info().await?;
        ds_2.sync()?;
        ds_0.broker.settle(vec![]).await?;
        ds_1.broker.settle(vec![]).await?;
        ds_2.broker.settle(vec![]).await?;
        assert!(ds_2
            .get_flo::<Realm>(&router.realm.global_id())
            .await
            .is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn test_cuckoo_propagation() -> Result<(), Box<dyn Error>> {
        start_logging_filter_level(vec!["flmodules::dht_storage"], log::LevelFilter::Info);

        let mut router = SimulStorage::new().await?;
        let mut ds_0 = router.node().await?;
        let mut ds_1 = router.node().await?;
        router.simul.send_node_info().await?;

        // Testing copy of flo in other node(s) when they are present during
        // the storage operation.
        ds_0.store_flo(router.realm.clone().into())?;
        router.sync_all().await?;

        let t0 = FloTesting::new_cuckoo(router.realm.realm_id(), "test0", Cuckoo::Duration(100000));
        let t1 = FloTesting::new_cuckoo(
            router.realm.realm_id(),
            "test1",
            Cuckoo::Parent(t0.flo_id()),
        );

        ds_0.store_flo(t0.flo().clone())?;
        ds_0.store_flo(t1.flo().clone())?;
        router.settle_all().await;

        log::info!("Cuckoo in ds_0");
        assert_eq!(1, ds_0.get_cuckoos(&t0.global_id()).await?.len());
        router.settle_all().await;

        log::info!("Cuckoo in ds_1");
        router.sync_all().await?;
        assert_eq!(1, ds_1.get_cuckoos(&t0.global_id()).await?.len());

        Ok(())
    }
}
