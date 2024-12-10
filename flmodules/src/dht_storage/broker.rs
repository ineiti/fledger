use flarch::{
    broker_io::{BrokerError, BrokerIO},
    data_storage::DataStorage,
    nodeids::U256,
    tasks::spawn_local,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use tokio::sync::watch;

use crate::{
    dht_routing::broker::{DHTRoutingIn, DHTRoutingOut},
    flo::dht::{DHTFlo, DHTStorageConfig},
};
use flarch::nodeids::NodeID;

use super::{
    core::{DHTStorageBucket, DHTStorageStorageSave},
    messages::{DHTStorageMessages, DHTStorageStats, InternIn, InternOut},
};

pub(super) const MODULE_NAME: &str = "DHTStorage";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DHTStorageIn {
    StoreValue(DHTFlo),
    ReadValue(U256),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DHTStorageOut {
    Value(DHTFlo),
    ValueMissing(U256),
    UpdateStorage(DHTStorageBucket),
    UpdateStats(DHTStorageStats),
}

/// This links the DHTStorage module with other modules, so that
/// all messages are correctly translated from one to the other.
/// For this example, it uses the RandomConnections module to communicate
/// with other nodes.
///
/// The [DHTStorage] holds the [Translate] and offers convenience methods
/// to interact with [Translate] and [DHTStorageMessage].
pub struct DHTStorage {
    /// Represents the underlying broker.
    pub broker: BrokerIO<DHTStorageIn, DHTStorageOut>,
    pub storage: watch::Receiver<DHTStorageBucket>,
}

impl DHTStorage {
    pub async fn start(
        mut ds: Box<dyn DataStorage + Send>,
        our_id: NodeID,
        dht_routing: BrokerIO<DHTRoutingIn, DHTRoutingOut>,
        config: DHTStorageConfig,
    ) -> Result<Self, Box<dyn Error>> {
        let str = ds.get(MODULE_NAME).unwrap_or("".into());
        let storage =
            DHTStorageStorageSave::from_str(&str).unwrap_or(DHTStorageBucket::new(our_id));
        let messages = DHTStorageMessages::new(storage.clone(), config);

        let mut dht_storage = BrokerIO::new();

        dht_storage.add_handler(Box::new(messages)).await?;
        dht_storage
            .add_translator_link(
                dht_routing,
                Box::new(|msg| match msg {
                    InternOut::Routing(dhtrouting_in) => Some(dhtrouting_in),
                    _ => None,
                }),
                Box::new(|msg| Some(InternIn::Routing(msg))),
            )
            .await?;

        let broker = BrokerIO::new();
        dht_storage
            .add_translator_direct(
                broker.clone(),
                Box::new(|msg| Some(InternIn::Storage(msg))),
                Box::new(|msg| match msg {
                    InternOut::Storage(dhtstorage_out) => Some(dhtstorage_out),
                    _ => None,
                }),
            )
            .await?;

        let (tx, storage) = watch::channel(storage);
        let (mut tap, _) = dht_storage.get_tap_out().await?;
        spawn_local(async move {
            loop {
                if let Some(InternOut::Storage(DHTStorageOut::UpdateStorage(sto))) =
                    tap.recv().await
                {
                    tx.send(sto.clone()).expect("updated storage");
                    if let Ok(val) = sto.to_yaml() {
                        ds.set(MODULE_NAME, &val).expect("updating storage");
                    }
                }
            }
        });

        Ok(DHTStorage { broker, storage })
    }

    pub fn store_dhtflo(&mut self, df: DHTFlo) -> Result<(), BrokerError> {
        self.broker.emit_msg_in(DHTStorageIn::StoreValue(df))
    }

    pub async fn read_dhtflo_local(&mut self, id: &U256) -> Option<DHTFlo> {
        self.storage.borrow().get(id)
    }

    pub async fn request_dhtflo_global(&mut self, id: &U256) -> Result<(), BrokerError> {
        self.broker.emit_msg_in(DHTStorageIn::ReadValue(*id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flo::testing::{new_ace, new_dht_flo_blob};
    use flarch::{data_storage::DataStorageTemp, start_logging_filter_level};

    #[tokio::test]
    async fn test_storage_simple() -> Result<(), Box<dyn Error>> {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let mut dht_routing = BrokerIO::new();
        let mut ds = DHTStorage::start(
            Box::new(DataStorageTemp::new()),
            NodeID::rnd(),
            dht_routing.clone(),
            DHTStorageConfig::default(),
        )
        .await?;

        let ace = new_ace()?;
        let df = new_dht_flo_blob(ace.clone(), "1234".into());
        ds.store_dhtflo(df.clone())?;
        ds.broker.settle(vec![]).await?;

        let (mut dht_out, _) = dht_routing.get_tap_in().await?;

        let stored = ds.read_dhtflo_local(&df.id()).await;
        assert!(stored.is_some());
        assert_eq!(df, stored.unwrap());
        ds.broker.settle(vec![]).await?;
        assert!(dht_out.try_recv().is_err());

        let stored = ds.read_dhtflo_local(&U256::rnd()).await;
        assert!(stored.is_none());
        ds.broker.settle(vec![]).await?;
        assert!(dht_out.try_recv().is_ok());

        Ok(())
    }
}
