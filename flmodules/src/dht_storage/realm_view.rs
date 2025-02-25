use crate::{
    dht_storage::{
        broker::DHTStorage,
        core::{Cuckoo, RealmConfig},
    },
    flo::{
        blob::{BlobID, FloBlobPage, FloBlobTag},
        flo::{FloError, FloID, FloWrapper},
        realm::{FloRealm, GlobalID, Realm, RealmID},
    },
};
use bytes::Bytes;
use flarch::broker::BrokerError;
use flcrypto::{
    access::Condition,
    signer::{Signer, SignerError},
};
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;

use super::broker::StorageError;

pub struct RealmView {
    dht_storage: DHTStorage,
    pub realm: FloRealm,
    pub pages: Vec<FloBlobPage>,
    pub tags: Vec<FloBlobTag>,
}

#[derive(Error, Debug)]
pub enum RVError {
    #[error("Didn't find a realm")]
    NoRealm,
    #[error("Couldn't find a page")]
    NoPage,
    #[error("Couldn't find a tag")]
    NoTag,
    #[error("Couldn't find or convert this flo")]
    NoSuchFlo,
    #[error("BrokerError({0})")]
    BrokerError(#[from] BrokerError),
    #[error("FloError({0})")]
    FloError(#[from] FloError),
    #[error("StorageError({0})")]
    StorageError(#[from] StorageError),
    #[error("SignerError({0})")]
    SignerError(#[from] SignerError),
}

impl RealmView {
    pub async fn new(mut dht_storage: DHTStorage) -> anyhow::Result<Self> {
        if let Some(id) = dht_storage.get_realm_ids().await?.first() {
            Self::new_from_id(dht_storage, id.clone()).await
        } else {
            Err(RVError::NoRealm.into())
        }
    }

    pub async fn new_from_id(mut dht_storage: DHTStorage, id: RealmID) -> anyhow::Result<Self> {
        let realm = Self::get_realm(&mut dht_storage, &id).await?;
        Self::new_from_realm(dht_storage, realm).await
    }

    pub async fn new_from_realm(dht_storage: DHTStorage, realm: FloRealm) -> anyhow::Result<Self> {
        let mut rv = Self {
            realm,
            pages: vec![],
            tags: vec![],
            dht_storage,
        };
        rv.get_realm_page().await?;
        rv.get_realm_tag().await?;
        Ok(rv)
    }

    pub async fn new_create_realm(
        dht_storage: DHTStorage,
        name: &str,
        cond: Condition,
        signers: &[&Signer],
    ) -> anyhow::Result<Self> {
        Self::new_create_realm_config(
            dht_storage,
            name,
            cond,
            RealmConfig {
                max_space: 1e6 as u64,
                max_flo_size: 1e4 as u32,
            },
            signers,
        )
        .await
    }

    pub async fn new_create_realm_config(
        mut dht_storage: DHTStorage,
        name: &str,
        cond: Condition,
        config: RealmConfig,
        signers: &[&Signer],
    ) -> anyhow::Result<Self> {
        let realm = FloRealm::new(name, cond, config, signers)?;
        dht_storage.store_flo(realm.clone().into())?;
        Self::new_from_realm(dht_storage, realm).await
    }

    pub fn create_http(
        &mut self,
        path: &str,
        content: String,
        parent: Option<BlobID>,
        cond: Condition,
        signers: &[&Signer],
    ) -> anyhow::Result<FloBlobPage> {
        Self::create_http_cuckoo(self, path, content, parent, cond, Cuckoo::None, signers)
    }

    pub fn create_http_cuckoo(
        &mut self,
        path: &str,
        content: String,
        parent: Option<BlobID>,
        cond: Condition,
        cuckoo: Cuckoo,
        signers: &[&Signer],
    ) -> anyhow::Result<FloBlobPage> {
        let fp = FloBlobPage::new_cuckoo(
            self.realm.realm_id(),
            cond,
            path,
            Bytes::from(content),
            parent,
            cuckoo,
            signers,
        )?;
        self.dht_storage.store_flo(fp.clone().into())?;
        Ok(fp)
    }

    pub async fn set_realm_http(&mut self, id: FloID, signers: &[&Signer]) -> anyhow::Result<()> {
        Ok(self.set_realm_service("http", id, signers).await?)
    }

    pub async fn set_realm_service(
        &mut self,
        name: &str,
        id: FloID,
        signers: &[&Signer],
    ) -> anyhow::Result<()> {
        let cond = self
            .dht_storage
            .convert(self.realm.cond(), &self.realm.realm_id())
            .await;
        self.realm = self
            .realm
            .edit_data_signers(cond, |r| r.set_service(name, id), signers)?;
        Ok(self.dht_storage.store_flo(self.realm.clone().into())?)
    }

    pub fn create_tag(
        &mut self,
        name: &str,
        parent: Option<BlobID>,
        cond: Condition,
        signers: &[&Signer],
    ) -> anyhow::Result<FloBlobTag> {
        Ok(Self::create_tag_cuckoo(
            self,
            name,
            parent,
            cond,
            Cuckoo::None,
            signers,
        )?)
    }

    pub fn create_tag_cuckoo(
        &mut self,
        name: &str,
        parent: Option<BlobID>,
        cond: Condition,
        cuckoo: Cuckoo,
        signers: &[&Signer],
    ) -> anyhow::Result<FloBlobTag> {
        let ft =
            FloBlobTag::new_cuckoo(self.realm.realm_id(), cond, name, parent, cuckoo, signers)?;
        self.dht_storage.store_flo(ft.clone().into())?;
        Ok(ft)
    }
    pub async fn set_realm_tag(&mut self, id: FloID, signers: &[&Signer]) -> anyhow::Result<()> {
        self.set_realm_service("tag", id, signers).await
    }

    pub async fn get_pages(&mut self, id: FloID) -> anyhow::Result<Vec<FloBlobPage>> {
        self.read_parent_cuckoo(id).await
    }

    pub async fn get_tags(&mut self, id: FloID) -> anyhow::Result<Vec<FloBlobTag>> {
        self.read_parent_cuckoo(id).await
    }

    async fn read_parent_cuckoo<T: Serialize + DeserializeOwned + Clone>(
        &mut self,
        parent: FloID,
    ) -> anyhow::Result<Vec<FloWrapper<T>>> {
        let mut res = vec![];
        for id in self
            .dht_storage
            .get_cuckoos(&GlobalID::new(self.realm.realm_id(), parent.clone()))
            .await
            .unwrap_or_default()
            .into_iter()
            .chain(std::iter::once(parent))
        {
            self.dht_storage
                .clone()
                .get_flo::<T>(&GlobalID::new(self.realm.realm_id(), id))
                .await
                .ok()
                .map(|fw| res.push(fw));
        }
        Ok(res)
    }

    async fn get_realm(dht_storage: &mut DHTStorage, id: &RealmID) -> anyhow::Result<FloRealm> {
        Ok(dht_storage.get_flo::<Realm>(&id.into()).await?)
    }

    async fn get_realm_page(&mut self) -> anyhow::Result<()> {
        if let Some(id) = self.realm.cache().get_services().get("http") {
            self.pages = self.get_pages(id.clone()).await?;
        }

        Ok(())
    }

    async fn get_realm_tag(&mut self) -> anyhow::Result<()> {
        if let Some(id) = self.realm.cache().get_services().get("tag") {
            self.tags = self.get_tags(id.clone()).await?;
        }

        Ok(())
    }
}
