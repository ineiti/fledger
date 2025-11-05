use async_recursion::async_recursion;
use flarch::{
    add_translator_direct, add_translator_link,
    broker::{Broker, BrokerError},
    data_storage::DataStorage,
    tasks::wait_ms,
};
use flcrypto::{
    access::{Condition, ConditionLink},
    signer::{SignerError, Verifier},
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    select,
    sync::{mpsc::UnboundedReceiver, watch},
};

use crate::{
    dht_router::broker::BrokerDHTRouter,
    flo::{
        crypto::BadgeCond,
        flo::{Flo, FloError, FloID, FloWrapper, UpdateCondSign},
        realm::{GlobalID, RealmID},
    },
    timer::{BrokerTimer, Timer},
};

use super::{
    core::{CoreError, DHTConfig, FloCuckoo},
    intern::{InternIn, InternOut, Messages, Stats},
    realm_view::RealmView,
};

pub(super) const MODULE_NAME: &str = "DHTStorage";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DHTStorageIn {
    StoreFlo(Flo),
    ReadFlo(GlobalID),
    ReadCuckooIDs(GlobalID),
    GetFlos,
    GetRealms,
    /// Ask all neighbors for their realms and Flos.
    SyncFromNeighbors,
    /// Ask all neighbors to sync with us.
    PropagateFlos,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DHTStorageOut {
    FloValue(FloCuckoo),
    FloValues(Vec<FloCuckoo>),
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
#[derive(Clone, Debug)]
pub struct DHTStorage {
    /// Represents the underlying broker.
    pub broker: Broker<DHTStorageIn, DHTStorageOut>,
    pub stats: watch::Receiver<Stats>,
    intern: Broker<InternIn, InternOut>,
    config: DHTConfig,
}

// unsafe impl Send for DHTStorage {}

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
        config: DHTConfig,
        timer: BrokerTimer,
        dht_router: BrokerDHTRouter,
    ) -> anyhow::Result<Self> {
        let (messages, stats) = Messages::new(ds, config.clone());
        let mut intern = Broker::new();
        intern.add_handler(Box::new(messages)).await?;

        add_translator_link!(
            intern,
            dht_router.clone(),
            InternIn::Routing,
            InternOut::Routing
        );

        let broker = Broker::new();
        add_translator_direct!(
            intern,
            broker.clone(),
            InternIn::Storage,
            InternOut::Storage
        );
        Timer::minute(
            timer,
            intern.clone(),
            InternIn::Storage(DHTStorageIn::SyncFromNeighbors),
        )
        .await?;

        Ok(DHTStorage {
            broker,
            stats,
            intern,
            config,
        })
    }

    pub fn store_flo(&mut self, flo: Flo) -> anyhow::Result<()> {
        Ok(self.broker.emit_msg_in(DHTStorageIn::StoreFlo(flo))?)
    }

    pub fn propagate(&mut self) -> anyhow::Result<()> {
        Ok(self
            .intern
            .emit_msg_in(InternIn::Storage(DHTStorageIn::PropagateFlos))?)
    }

    pub async fn get_realm_ids(&mut self) -> anyhow::Result<Vec<RealmID>> {
        Ok(self
            .send_wait(DHTStorageIn::GetRealms, &|msg| match msg {
                DHTStorageOut::RealmIDs(realms) => Some(realms),
                _ => None,
            })
            .await?)
    }

    pub async fn get_flo<T: Serialize + DeserializeOwned + Clone>(
        &mut self,
        id: &GlobalID,
    ) -> anyhow::Result<FloWrapper<T>> {
        Ok(self.get_flo_timeout(id, self.config.timeout).await?)
    }

    pub async fn get_flo_timeout<T: Serialize + DeserializeOwned + Clone>(
        &mut self,
        id: &GlobalID,
        timeout: u64,
    ) -> anyhow::Result<FloWrapper<T>> {
        // log::info!(
        //     "Getting {}: {}/{}",
        //     std::any::type_name::<T>(),
        //     id.realm_id(),
        //     id.flo_id()
        // );
        Ok(self
            .send_wait_timeout(
                DHTStorageIn::ReadFlo(id.clone()),
                &|msg| match msg {
                    DHTStorageOut::FloValue((flo, _)) => (flo.global_id() == *id).then_some(flo),
                    _ => None,
                },
                timeout,
            )
            .await?
            .try_into()?)
    }

    pub async fn get_flos(&mut self) -> anyhow::Result<Vec<Flo>> {
        Ok(self
            .send_wait_timeout(
                DHTStorageIn::GetFlos,
                &|msg| match msg {
                    DHTStorageOut::FloValues(flos) => {
                        Some(flos.iter().map(|f| f.0.clone()).collect::<Vec<_>>())
                    }
                    _ => None,
                },
                1000,
            )
            .await?)
    }

    pub async fn get_cuckoos(&mut self, id: &GlobalID) -> anyhow::Result<Vec<FloID>> {
        Ok(self
            .send_wait(DHTStorageIn::ReadCuckooIDs(id.clone()), &|msg| match msg {
                DHTStorageOut::CuckooIDs(rid, ids) => (&rid == id).then_some(ids),
                _ => None,
            })
            .await?)
    }

    pub fn sync(&mut self) -> anyhow::Result<()> {
        Ok(self.broker.emit_msg_in(DHTStorageIn::SyncFromNeighbors)?)
    }

    #[async_recursion(?Send)]
    pub async fn convert(&mut self, cl: &ConditionLink, rid: &RealmID) -> Condition {
        match cl {
            ConditionLink::Verifier(ver) => self
                .get_flo::<Verifier>(&rid.global_id((*Flo::calc_flo_id(&*ver)).into()))
                .await
                .map(|v| Condition::Verifier(v.cache().clone()))
                .unwrap_or(Condition::NotAvailable),
            ConditionLink::Badge(version) => {
                if let Ok(b) = self
                    .get_flo::<BadgeCond>(&rid.global_id((*version.get_id()).into()))
                    .await
                {
                    let bl = b.badge_link();
                    Condition::Badge(
                        version.clone(),
                        flcrypto::access::Badge {
                            id: bl.id,
                            version: bl.version,
                            condition: Box::new(self.convert(&bl.condition, rid).await),
                        },
                    )
                } else {
                    Condition::NotAvailable
                }
            }
            ConditionLink::NofT(thr, vec) => {
                let mut conds = vec![];
                for cond in vec {
                    conds.push(self.convert(cond, rid).await)
                }
                Condition::NofT(*thr, conds)
            }
            ConditionLink::Pass => Condition::Pass,
            ConditionLink::Fail => Condition::Fail,
        }
    }

    pub async fn start_sign_cond<T: Serialize + DeserializeOwned + Clone>(
        &mut self,
        fw: &FloWrapper<T>,
        cl: &ConditionLink,
    ) -> anyhow::Result<UpdateCondSign> {
        let rid = fw.realm_id();
        let cond_old = self.convert(fw.cond(), &rid).await;
        let cond_new = self.convert(cl, &rid).await;
        fw.start_sign_cond(cond_old, cond_new)
    }

    pub async fn get_realm_view(&mut self, rid: RealmID) -> anyhow::Result<RealmView> {
        Ok(RealmView::new_from_id(self.clone(), rid).await?)
    }

    async fn send_wait<T>(
        &mut self,
        msg_in: DHTStorageIn,
        check: &(dyn Fn(DHTStorageOut) -> Option<T> + Sync),
    ) -> anyhow::Result<T> {
        Ok(self
            .send_wait_timeout(msg_in, check, self.config.timeout)
            .await?)
    }

    async fn send_wait_timeout<T>(
        &mut self,
        msg_in: DHTStorageIn,
        check: &(dyn Fn(DHTStorageOut) -> Option<T> + Sync),
        timeout: u64,
    ) -> anyhow::Result<T> {
        let (mut tap, tap_id) = self.intern.get_tap_out().await?;
        self.intern.emit_msg_in(InternIn::Storage(msg_in))?;
        let res = select! {
            _ = wait_ms(timeout) => Err(StorageError::TimeoutError.into()),
            res = Self::wait_tap(&mut tap, check) => res
        };
        self.intern.remove_subsystem(tap_id).await?;
        res
    }

    async fn wait_tap<T>(
        tap: &mut UnboundedReceiver<InternOut>,
        check: &(dyn Fn(DHTStorageOut) -> Option<T> + Sync),
    ) -> anyhow::Result<T> {
        while let Some(msg_int) = tap.recv().await {
            if let InternOut::Storage(msg_out) = msg_int {
                if let Some(res) = check(msg_out) {
                    return Ok(res);
                }
            }
        }
        Err(anyhow::anyhow!("Channel closed while waiting"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        dht_router::{broker::DHTRouter, kademlia::Config},
        dht_storage::core::{Cuckoo, RealmConfig},
        flo::realm::{FloRealm, Realm},
        testing::{
            network_simul::NetworkSimul,
            wallet::{FloTesting, Testing, Wallet},
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
        async fn new() -> anyhow::Result<Self> {
            let mut wallet = Wallet::new();
            Ok(Self {
                simul: NetworkSimul::new().await?,
                tick: Timer::simul(),
                nodes: vec![],
                realm: FloRealm::new(
                    "root",
                    wallet.get_badge_cond(),
                    RealmConfig {
                        max_space: 1e6 as u64,
                        max_flo_size: 1e4 as u32,
                    },
                    &[&wallet.get_signer()],
                )?,
                _wallet: wallet,
            })
        }

        async fn node(&mut self) -> anyhow::Result<DHTStorage> {
            let n = self.simul.new_node().await?;
            let id = n.config.info.get_id();
            let dht_router =
                DHTRouter::start(Config::default(id), self.tick.broker.clone(), n.router).await?;
            let n_s = DHTStorage::start(
                Box::new(DataStorageTemp::new()),
                DHTConfig::default(n.config.info.get_id()),
                self.tick.broker.clone(),
                dht_router.broker.clone(),
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

        async fn sync_all(&mut self) -> anyhow::Result<()> {
            for n in &mut self.nodes {
                n.sync()?;
            }
            self.settle_all().await;
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_realm_propagation() -> anyhow::Result<()> {
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
        router.settle_all().await;
        assert!(ds_2
            .get_flo::<Realm>(&router.realm.global_id())
            .await
            .is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn test_cuckoo_propagation() -> anyhow::Result<()> {
        start_logging_filter_level(vec!["flmodules::dht_storage"], log::LevelFilter::Info);

        let mut router = SimulStorage::new().await?;
        let mut ds_0 = router.node().await?;
        let mut ds_1 = router.node().await?;
        router.simul.send_node_info().await?;

        // Testing copy of flo in other node(s) when they are present during
        // the storage operation.
        ds_0.store_flo(router.realm.clone().into())?;
        router.sync_all().await?;

        let t0 = FloTesting::new_cuckoo(
            router.realm.realm_id(),
            "test0",
            Cuckoo::Duration(100000),
            &router._wallet.get_signer(),
        );
        let t1 = FloTesting::new_cuckoo(
            router.realm.realm_id(),
            "test1",
            Cuckoo::Parent(t0.flo_id()),
            &router._wallet.get_signer(),
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

        // Testing copy of flo in other node(s) who join later.
        log::info!("Copying flos to new nodes");
        let mut ds_2 = router.node().await?;

        router.simul.send_node_info().await?;
        ds_2.sync()?;
        router.settle_all().await;
        assert!(ds_2.get_flo::<Testing>(&t0.flo().global_id()).await.is_ok());
        assert!(ds_2.get_flo::<Testing>(&t1.flo().global_id()).await.is_ok());

        Ok(())
    }
}
