use std::collections::HashMap;

use crate::{
    dht_storage::{
        broker::DHTStorage,
        core::{Cuckoo, RealmConfig},
    },
    flo::{
        blob::{BlobID, BlobPage, BlobTag, FloBlobPage, FloBlobTag},
        flo::{FloError, FloWrapper},
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

#[derive(Debug, Clone)]
pub struct RealmView {
    dht_storage: DHTStorage,
    pub realm: FloRealm,
    pub pages: Option<RealmStorage<BlobPage>>,
    pub tags: Option<RealmStorage<BlobTag>>,
}

#[derive(Debug, Clone)]
pub struct RealmStorage<T> {
    realm: RealmID,
    pub root: BlobID,
    pub storage: HashMap<BlobID, FloWrapper<T>>,
    pub cuckoos: HashMap<BlobID, Vec<BlobID>>,
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
        let realm = dht_storage.get_flo::<Realm>(&(&id).into()).await?;
        Self::new_from_realm(dht_storage, realm).await
    }

    pub async fn new_from_realm(dht_storage: DHTStorage, realm: FloRealm) -> anyhow::Result<Self> {
        let mut rv = Self {
            realm,
            pages: None,
            tags: None,
            dht_storage,
        };
        rv.pages = rv.get_realm_storage("http").await?;
        rv.tags = rv.get_realm_storage("tag").await?;
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

    pub async fn set_realm_http(&mut self, id: BlobID, signers: &[&Signer]) -> anyhow::Result<()> {
        Ok(self.set_realm_service("http", id, signers).await?)
    }

    pub async fn set_realm_service(
        &mut self,
        name: &str,
        id: BlobID,
        signers: &[&Signer],
    ) -> anyhow::Result<()> {
        let cond = self
            .dht_storage
            .convert(self.realm.cond(), &self.realm.realm_id())
            .await;
        self.realm =
            self.realm
                .edit_data_signers(cond, |r| r.set_service(name, (*id).into()), signers)?;
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
    pub async fn set_realm_tag(&mut self, id: BlobID, signers: &[&Signer]) -> anyhow::Result<()> {
        self.set_realm_service("tag", id, signers).await
    }

    pub async fn read_pages(&mut self, id: BlobID) -> anyhow::Result<()> {
        if let Some(page) = self.pages.as_mut() {
            page.get_blobs(&mut self.dht_storage, id).await?;
        }
        Ok(())
    }

    pub async fn read_tags(&mut self, id: BlobID) -> anyhow::Result<()> {
        if let Some(tag) = self.tags.as_mut() {
            tag.get_blobs(&mut self.dht_storage, id).await?;
        }
        Ok(())
    }

    pub async fn update_pages(&mut self) -> anyhow::Result<()> {
        if let Some(pages) = self.pages.as_mut() {
            pages.update_blobs(&mut self.dht_storage).await?;
        }
        Ok(())
    }

    pub async fn update_tags(&mut self) -> anyhow::Result<()> {
        if let Some(tags) = self.tags.as_mut() {
            tags.update_blobs(&mut self.dht_storage).await?;
        }
        Ok(())
    }

    async fn get_realm_storage<T: Serialize + DeserializeOwned + Clone>(
        &mut self,
        service: &str,
    ) -> anyhow::Result<Option<RealmStorage<T>>> {
        Ok(
            if let Some(id) = self.realm.cache().get_services().get(service) {
                Some(
                    RealmStorage::from_root(
                        &mut self.dht_storage,
                        self.realm.realm_id().global_id(id.clone()),
                    )
                    .await?,
                )
            } else {
                None
            },
        )
    }
}

impl<T: Serialize + DeserializeOwned + Clone> RealmStorage<T> {
    async fn from_root(ds: &mut DHTStorage, root: GlobalID) -> anyhow::Result<Self> {
        let mut rs = Self {
            realm: root.realm_id().clone(),
            root: (*root.flo_id().clone()).into(),
            storage: HashMap::new(),
            cuckoos: HashMap::new(),
        };
        rs.get_blobs(ds, rs.root.clone()).await?;

        Ok(rs)
    }

    async fn get_blobs(&mut self, ds: &mut DHTStorage, bid: BlobID) -> anyhow::Result<()> {
        for id in ds
            .get_cuckoos(&self.realm.global_id((*bid).into()))
            .await
            .unwrap_or_default()
            .into_iter()
            .chain(std::iter::once((*bid).into()))
        {
            let blob = ds.get_flo::<T>(&self.realm.global_id(id.clone())).await?;
            self.storage.insert((*blob.flo_id()).into(), blob.clone());
            if id != blob.flo_id() {
                let new_id: BlobID = (*blob.flo_id()).into();
                let cuckoos = self.cuckoos.entry(new_id.clone()).or_insert(vec![]);
                if !cuckoos.contains(&new_id) {
                    cuckoos.push(new_id);
                }
            }
        }

        Ok(())
    }

    async fn update_blobs(&mut self, ds: &mut DHTStorage) -> anyhow::Result<()> {
        let bids = self.storage.keys().cloned().collect::<Vec<_>>();
        for bid in bids {
            self.get_blobs(ds, bid).await?;
        }

        Ok(())
    }
}
