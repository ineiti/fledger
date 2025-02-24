use crate::{
    dht_storage::{
        broker::DHTStorage,
        core::{Cuckoo, RealmConfig},
    },
    flo::{
        blob::{BlobID, FloBlobPage, FloBlobTag},
        crypto::Rules,
        flo::{FloError, FloID, FloWrapper},
        realm::{FloRealm, GlobalID, Realm, RealmID},
    },
};
use bytes::Bytes;
use flarch::broker::BrokerError;
use flcrypto::signer::{Signer, SignerError};
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
    pub async fn new(mut dht_storage: DHTStorage) -> Result<Self, RVError> {
        if let Some(id) = dht_storage.get_realm_ids().await?.first() {
            Self::new_from_id(dht_storage, id.clone()).await
        } else {
            Err(RVError::NoRealm)
        }
    }

    pub async fn new_from_id(mut dht_storage: DHTStorage, id: RealmID) -> Result<Self, RVError> {
        let realm = Self::get_realm(&mut dht_storage, &id).await?;
        Self::new_from_realm(dht_storage, realm).await
    }

    pub async fn new_from_realm(dht_storage: DHTStorage, realm: FloRealm) -> Result<Self, RVError> {
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
        rules: Rules,
    ) -> Result<Self, RVError> {
        Self::new_create_realm_config(
            dht_storage,
            name,
            rules,
            RealmConfig {
                max_space: 1e6 as u64,
                max_flo_size: 1e4 as u32,
            },
        )
        .await
    }

    pub async fn new_create_realm_config(
        mut dht_storage: DHTStorage,
        name: &str,
        rules: Rules,
        config: RealmConfig,
    ) -> Result<Self, RVError> {
        let realm = FloRealm::new(name, rules, config)?;
        dht_storage.store_flo(realm.clone().into())?;
        Self::new_from_realm(dht_storage, realm).await
    }

    pub fn create_http(
        &mut self,
        path: &str,
        content: String,
        parent: Option<BlobID>,
        rules: Option<Rules>,
    ) -> Result<FloBlobPage, RVError> {
        Self::create_http_cuckoo(self, path, content, parent, rules, Cuckoo::None)
    }

    pub fn create_http_cuckoo(
        &mut self,
        path: &str,
        content: String,
        parent: Option<BlobID>,
        rules: Option<Rules>,
        cuckoo: Cuckoo,
    ) -> Result<FloBlobPage, RVError> {
        let fp = FloBlobPage::new_cuckoo(
            self.realm.realm_id(),
            rules.unwrap_or_else(|| self.realm.rules().clone()),
            path,
            Bytes::from(content),
            parent,
            cuckoo,
        )?;
        self.dht_storage.store_flo(fp.clone().into())?;
        Ok(fp)
    }

    pub async fn set_realm_http(
        &mut self,
        id: FloID,
        signers: &[&dyn Signer],
    ) -> Result<(), RVError> {
        self.set_realm_service("http", id, signers).await
    }

    pub async fn set_realm_service(
        &mut self,
        name: &str,
        id: FloID,
        signers: &[&dyn Signer],
    ) -> Result<(), RVError> {
        let update = self.realm.edit(|r| r.set_service(name, id));
        let mut u_s = self
            .dht_storage
            .get_update_sign(&self.realm, update, None)
            .await?;
        for &signer in signers {
            u_s.sign(signer)?;
        }
        self.realm.apply_update(u_s)?;
        Ok(self.dht_storage.store_flo(self.realm.clone().into())?)
    }

    pub fn create_tag(
        &mut self,
        name: &str,
        parent: Option<BlobID>,
        rules: Option<Rules>,
    ) -> Result<FloBlobTag, RVError> {
        Self::create_tag_cuckoo(self, name, parent, rules, Cuckoo::None)
    }

    pub fn create_tag_cuckoo(
        &mut self,
        name: &str,
        parent: Option<BlobID>,
        rules: Option<Rules>,
        cuckoo: Cuckoo,
    ) -> Result<FloBlobTag, RVError> {
        let ft = FloBlobTag::new_cuckoo(
            self.realm.realm_id(),
            rules.unwrap_or_else(|| self.realm.rules().clone()),
            name,
            parent,
            cuckoo,
        )?;
        self.dht_storage.store_flo(ft.clone().into())?;
        Ok(ft)
    }
    pub async fn set_realm_tag(
        &mut self,
        id: FloID,
        signers: &[&dyn Signer],
    ) -> Result<(), RVError> {
        self.set_realm_service("tag", id, signers).await
    }

    pub async fn get_pages(&mut self, id: FloID) -> Result<Vec<FloBlobPage>, RVError> {
        self.read_parent_cuckoo(id).await
    }

    pub async fn get_tags(&mut self, id: FloID) -> Result<Vec<FloBlobTag>, RVError> {
        self.read_parent_cuckoo(id).await
    }

    async fn read_parent_cuckoo<T: Serialize + DeserializeOwned + Clone>(
        &mut self,
        parent: FloID,
    ) -> Result<Vec<FloWrapper<T>>, RVError> {
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

    async fn get_realm(dht_storage: &mut DHTStorage, id: &RealmID) -> Result<FloRealm, RVError> {
        Ok(dht_storage.get_flo::<Realm>(&id.into()).await?)
    }

    async fn get_realm_page(&mut self) -> Result<(), RVError> {
        if let Some(id) = self.realm.cache().get_services().get("http") {
            self.pages = self.get_pages(id.clone()).await?;
        }

        Ok(())
    }

    async fn get_realm_tag(&mut self) -> Result<(), RVError> {
        if let Some(id) = self.realm.cache().get_services().get("tag") {
            self.tags = self.get_tags(id.clone()).await?;
        }

        Ok(())
    }
}
