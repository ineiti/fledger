use std::{collections::HashMap, iter::once};

use crate::{
    dht_storage::{
        broker::DHTStorage,
        core::{Cuckoo, RealmConfig},
    },
    flo::{
        blob::{BlobFamily, BlobID, BlobPage, BlobPath, BlobTag, FloBlobPage, FloBlobTag},
        flo::{FloError, FloWrapper},
        realm::{FloRealm, GlobalID, Realm, RealmID},
    },
};
use bytes::Bytes;
use flarch::broker::BrokerError;
use flcrypto::{
    access::Condition,
    signer::{Signer, SignerError, SignerTrait, VerifierTrait},
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
    pub async fn new_first(mut dht_storage: DHTStorage) -> anyhow::Result<Self> {
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

    pub async fn create_http(
        &mut self,
        path: &str,
        content: String,
        parent: Option<BlobID>,
        cond: Condition,
        signers: &[&Signer],
    ) -> anyhow::Result<FloBlobPage> {
        Self::create_http_cuckoo(self, path, content, parent, cond, Cuckoo::None, signers).await
    }

    pub async fn create_http_cuckoo(
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
            parent.clone(),
            cuckoo,
            signers,
        )?;
        self.dht_storage.store_flo(fp.clone().into())?;
        if let Some(parent_page) = parent.and_then(|p| {
            self.pages
                .as_mut()
                .and_then(|pages| pages.storage.get_mut(&p))
        }) {
            let cond = self
                .dht_storage
                .convert(parent_page.cond(), &self.realm.realm_id())
                .await;
            let verifiers = signers
                .iter()
                .map(|sig| sig.verifier().get_id())
                .collect::<Vec<_>>();
            if cond != Condition::Pass && !cond.can_verify(&verifiers.iter().collect::<Vec<_>>()) {
                log::warn!("Cannot update parent with this signer - so this page will be difficult to find!");
            } else {
                self.dht_storage.store_flo(parent_page.edit_data_signers(
                    cond,
                    |page| page.0.add_child(fp.blob_id()),
                    signers,
                )?.into())?;
            }
        } else {
            log::warn!("Couldn't get parent page to update 'children'");
        }
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
            page.update_blobs_from_ds(&mut self.dht_storage, &id)
                .await?;
        }
        Ok(())
    }

    pub async fn read_tags(&mut self, id: BlobID) -> anyhow::Result<()> {
        if let Some(tag) = self.tags.as_mut() {
            tag.update_blobs_from_ds(&mut self.dht_storage, &id).await?;
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

    async fn get_realm_storage<T: Serialize + DeserializeOwned + Clone + std::fmt::Debug>(
        &mut self,
        service: &str,
    ) -> anyhow::Result<Option<RealmStorage<T>>>
    where
        FloWrapper<T>: BlobPath + BlobFamily,
    {
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

    pub fn get_page_path(&self, path: &str) -> Option<&FloBlobPage> {
        self.pages.as_ref().and_then(|page| page.get_path(path))
    }
}

impl<T: Serialize + DeserializeOwned + Clone + std::fmt::Debug> RealmStorage<T>
where
    FloWrapper<T>: BlobPath + BlobFamily,
{
    pub fn get_path(&self, path: &str) -> Option<&FloWrapper<T>> {
        self.get_path_internal(path.split('/').collect::<Vec<_>>(), vec![&self.root])
    }

    /// Returns the Blob and its cuckoos.
    /// If no blob with this ID is stored, it starts by fetching this blob and its cuckoos.
    /// If the blob doesn't exist, but its cuckoos, then it will try to fetch the blob, and then
    /// return either the blob and its cuckoos, or just the cuckoos.
    pub async fn get_cuckoos(
        &mut self,
        ds: &mut DHTStorage,
        bid: BlobID,
    ) -> anyhow::Result<Vec<&FloWrapper<T>>> {
        if self.storage.get(&bid).is_none() {
            self.update_blobs_from_ds(ds, &bid).await?;
        }
        Ok(once(&bid)
            .chain(self.cuckoos.get(&bid).unwrap_or(&Vec::new()))
            .filter_map(|id| self.storage.get(id))
            .collect::<Vec<_>>())
    }

    fn get_path_internal(&self, parts: Vec<&str>, curr: Vec<&BlobID>) -> Option<&FloWrapper<T>> {
        let ids = curr
            .into_iter()
            .flat_map(|id| once(id).chain(self.cuckoos.get(id).into_iter().flatten()))
            .collect::<Vec<_>>();
        if let Some(part) = parts.split_first() {
            if let Some(f) = ids
                .iter()
                .filter_map(|id| self.storage.get(id))
                .find(|f| f.get_path().map(|p| p == part.0).unwrap_or(false))
            {
                if part.1.len() == 0 {
                    return Some(f);
                } else {
                    let children = f.get_children();
                    if children.is_empty() {
                        return None;
                    }
                    return self.get_path_internal(part.1.to_vec(), children.iter().collect());
                }
            }
        }
        None
    }

    async fn from_root(ds: &mut DHTStorage, root: GlobalID) -> anyhow::Result<Self> {
        let mut rs = Self {
            realm: root.realm_id().clone(),
            root: (*root.flo_id().clone()).into(),
            storage: HashMap::new(),
            cuckoos: HashMap::new(),
        };
        rs.update_blobs_from_ds(ds, &rs.root.clone()).await?;
        Ok(rs)
    }

    async fn update_blobs_from_ds(
        &mut self,
        ds: &mut DHTStorage,
        bid: &BlobID,
    ) -> anyhow::Result<()> {
        for id in ds
            .get_cuckoos(&self.realm.global_id((**bid).into()))
            .await
            .unwrap_or_default()
            .into_iter()
            .chain(std::iter::once((**bid).into()))
        {
            let blob = ds.get_flo::<T>(&self.realm.global_id(id.clone())).await?;
            self.storage.insert((*blob.flo_id()).into(), blob.clone());
            if id != blob.flo_id() {
                let new_id: BlobID = (*blob.flo_id()).into();
                let cuckoos = self.cuckoos.entry(new_id.clone()).or_insert_with(Vec::new);
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
            self.update_blobs_from_ds(ds, &bid).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use super::*;

    fn add_page(
        rs: &mut RealmStorage<BlobPage>,
        path: &str,
        parent: Option<BlobID>,
        cuckoo: Cuckoo,
    ) -> FloBlobPage {
        let page = FloBlobPage::new_cuckoo(
            rs.realm.clone(),
            Condition::Pass,
            path.into(),
            Bytes::from("<html></html>"),
            parent.clone(),
            cuckoo,
            &[],
        )
        .expect("creating new page");
        if let Some(c) = page.flo().flo_config().cuckoo_parent() {
            rs.cuckoos
                .entry((**c).into())
                .or_insert_with(Vec::new)
                .push(page.blob_id());
        }
        if let Some(p) = &parent {
            rs.storage.entry((**p).into()).and_modify(|e| {
                let mut children = e.get_children();
                children.push(page.blob_id());
                e.set_children(children);
            });
        }
        rs.storage.insert(page.blob_id(), page.clone());
        page
    }

    #[test]
    fn get_path() -> anyhow::Result<()> {
        let mut rs: RealmStorage<BlobPage> = RealmStorage {
            realm: RealmID::from_str("00")?,
            root: BlobID::from_str("00")?,
            storage: HashMap::new(),
            cuckoos: HashMap::new(),
        };
        let root = add_page(&mut rs, "danu", None, Cuckoo::Duration(1000));
        rs.root = root.blob_id();
        let root_c0 = add_page(&mut rs, "dahu", None, Cuckoo::Parent(root.flo_id()));

        let root_blog = add_page(&mut rs, "blog", Some(root.blob_id()), Cuckoo::None);
        let root_c0_article = add_page(
            &mut rs,
            "montagne",
            Some(root_c0.blob_id()),
            Cuckoo::Parent(root_c0.flo_id()),
        );

        assert_eq!(None, rs.get_path("/"));
        assert_eq!(None, rs.get_path("blob"));
        // Cannot compare root itself, as the rs.root got some children...
        assert_eq!(
            Some(root.blob_id()),
            rs.get_path("danu").map(|f| f.blob_id())
        );
        assert_eq!(
            Some(root_c0.blob_id()),
            rs.get_path("dahu").map(|f| f.blob_id())
        );
        assert_eq!(
            Some(root_blog.blob_id()),
            rs.get_path("danu/blog").map(|f| f.blob_id())
        );
        assert_eq!(
            Some(root_c0_article.blob_id()),
            rs.get_path("dahu/montagne").map(|f| f.blob_id())
        );

        Ok(())
    }
}
