use std::{
    collections::{HashMap, HashSet},
    iter::once,
};

use crate::{
    dht_storage::{
        broker::{DHTStorage, DHTStorageOut, StorageError},
        core::{Cuckoo, RealmConfig},
    },
    flo::{
        blob::{
            BlobFamily, BlobID, BlobPage, BlobPath, BlobPathFamily, BlobTag, FloBlobPage,
            FloBlobTag,
        },
        flo::{FloError, FloID, FloWrapper},
        realm::{FloRealm, GlobalID, Realm, RealmID},
    },
};
use bytes::Bytes;
use flarch::broker::BrokerError;
use flcrypto::{
    access::Condition,
    signer::{Signer, SignerError, SignerTrait, VerifierTrait},
};
use regex::Regex;
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;
use tokio::sync::mpsc::UnboundedReceiver;

/// A RealmView is a wrapper for the dht_storage, FloRealm, FloBlob{Page,Tag}.
/// It offers a simpler interface to interact with these elements using paths
/// as well as IDs.
/// If you want to have a RealmView for a new Realm, please use [RealmViewBuilder].
#[derive(Debug, Clone)]
pub struct RealmView {
    dht_storage: DHTStorage,
    pub realm: FloRealm,
    pub pages: RealmStorage<BlobPage>,
    pub tags: RealmStorage<BlobTag>,
}

#[derive(Debug, Clone)]
pub struct RealmStorage<T> {
    realm: RealmID,
    ds: DHTStorage,
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
    /// Uses the first realm which has a page and a tag service.
    pub async fn new_first(mut dht_storage: DHTStorage) -> anyhow::Result<Self> {
        for id in dht_storage.get_realm_ids().await? {
            match Self::new_from_id(dht_storage.clone(), id.clone()).await {
                Ok(rv) => return Ok(rv),
                Err(e) => log::warn!("Found partial realm {id}: {e:?}"),
            }
        }
        Err(RVError::NoRealm.into())
    }

    /// Uses the realm with the given ID.
    ///
    /// Error: if there is no page and/or tag service in the given realm.
    pub async fn new_from_id(mut dht_storage: DHTStorage, id: RealmID) -> anyhow::Result<Self> {
        let realm = dht_storage.get_flo::<Realm>(&(&id).into()).await?;
        Self::new_from_realm(dht_storage, realm).await
    }

    /// Uses the given realm.
    ///
    /// Error: if there is no page and/or tag service in the given realm.
    pub async fn new_from_realm(dht_storage: DHTStorage, realm: FloRealm) -> anyhow::Result<Self> {
        let pages = RealmStorage::from_service(dht_storage.clone(), &realm, "http")
            .await?
            .ok_or(anyhow::anyhow!(
                "Didn't find the root page for {}",
                realm.realm_id()
            ))?;
        let tags = RealmStorage::from_service(dht_storage.clone(), &realm, "tag")
            .await?
            .ok_or(anyhow::anyhow!("Didn't find the root tag"))?;
        Ok(Self {
            realm,
            pages,
            tags,
            dht_storage,
        })
    }

    /// Creates a new FloBlobPage for the given path and with an optional parent.
    /// If no parent is given, or if the parent isn't a cuckoo to a known page,
    /// it will be difficult to find this page.
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

    /// Creates a new FloBlobPage for the given path and with an optional parent.
    /// The 'Cuckoo' can either link to an existing page, or offer an attach point for
    /// other pages.
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
            cuckoo.clone(),
            signers,
        )?;
        self.store_page(fp.clone())?;
        if let Some(parent_page) = parent.clone().and_then(|p| self.pages.storage.get_mut(&p)) {
            let cond = self
                .dht_storage
                .convert(parent_page.cond(), &self.realm.realm_id())
                .await;
            let verifiers = signers
                .iter()
                .map(|sig| sig.verifier().get_id())
                .collect::<Vec<_>>();
            if !cond.can_sign(&verifiers.iter().collect::<Vec<_>>()) {
                if !matches!(cuckoo, Cuckoo::Parent(_)) {
                    log::warn!("Cannot update parent with this signer - so this page will be difficult to find!");
                } else {
                    log::warn!("Cannot update parent with this signer");
                }
            } else {
                let parent_update = parent_page.edit_data_signers(
                    cond,
                    |page| page.add_child(fp.blob_id()),
                    signers,
                )?;
                self.store_page(parent_update)?;
            }
        } else if parent.is_some() {
            log::warn!("Couldn't get parent page to update 'children'");
        }
        Ok(fp)
    }

    pub async fn set_realm_http(&mut self, id: BlobID, signers: &[&Signer]) -> anyhow::Result<()> {
        Ok(self.set_realm_service("http", id, signers).await?)
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
        self.store_tag(ft.clone())?;
        Ok(ft)
    }

    pub async fn set_realm_tag(&mut self, id: BlobID, signers: &[&Signer]) -> anyhow::Result<()> {
        self.set_realm_service("tag", id, signers).await
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

    pub async fn get_realm_service(&self, name: &str) -> Option<&FloID> {
        self.realm.cache().get_services().get(name)
    }

    pub async fn fetch_page(&mut self, id: BlobID) -> anyhow::Result<()> {
        self.pages.update_cuckoos(&id).await
    }

    pub async fn fetch_tag(&mut self, id: BlobID) -> anyhow::Result<()> {
        self.tags.update_cuckoos(&id).await
    }

    pub async fn update_pages(&mut self) -> anyhow::Result<()> {
        self.pages.update_blobs().await
    }

    pub async fn update_tags(&mut self) -> anyhow::Result<()> {
        self.tags.update_blobs().await
    }

    pub fn get_blob_path_family(&self, id: &BlobID) -> Option<&dyn BlobPathFamily> {
        if let Some(page) = self.pages.storage.get(id) {
            Some(page as &dyn BlobPathFamily)
        } else if let Some(tag) = self.tags.storage.get(id) {
            Some(tag as &dyn BlobPathFamily)
        } else {
            None
        }
    }

    pub fn get_full_path_id(&self, id: &BlobID) -> anyhow::Result<String> {
        if let Some(page) = self.pages.storage.get(id) {
            self.get_full_path_blob(page)
        } else if let Some(tag) = self.tags.storage.get(id) {
            self.get_full_path_blob(tag)
        } else {
            Err(anyhow::anyhow!("Didn't find page or tag with this ID"))
        }
    }

    pub fn get_full_path_blob<T: BlobPathFamily>(&self, b: &T) -> anyhow::Result<String> {
        let path = b
            .get_path()
            .ok_or(anyhow::anyhow!("Didn't find path in blob"))?;
        if let Some(p) = b.get_parents().first() {
            return Ok(format!("{}/{path}", self.get_full_path_id(p)?));
        }
        Ok(format!("/{path}"))
    }

    pub fn get_page_from_path(&self, path: &str) -> anyhow::Result<&FloBlobPage> {
        self.pages
            .get_path(path)
            .map_err(|e| anyhow::anyhow!(format!("Didn't find page with path '{path}': {e:?}")))
    }

    /// Returns the parent page and the remaining path.
    /// For example, given the path "/root/foo/new_page", it will search for the
    /// parent at "/root/foo", and return "new_page".
    pub fn get_page_parent_remaining(&self, path: &str) -> anyhow::Result<(FloBlobPage, String)> {
        let re = Regex::new(r"^/(.*)/([^/]+)$")?;
        let (root, page) = re
            .captures(path)
            .map(|caps| (caps.get(1).unwrap().as_str(), caps.get(2).unwrap().as_str()))
            .ok_or(anyhow::anyhow!("Not correct path format"))?;
        Ok((self.get_page_from_path(root)?.clone(), page.to_string()))
    }

    pub fn get_root_tag(&self) -> Option<&FloBlobTag> {
        self.tags.storage.get(&self.tags.root)
    }

    pub fn get_root_page(&self) -> Option<&FloBlobPage> {
        self.pages.storage.get(&self.pages.root)
    }

    pub async fn update_page(
        &mut self,
        id: &BlobID,
        edit: impl FnOnce(&mut BlobPage),
        signers: &[&Signer],
    ) -> anyhow::Result<FloBlobPage> {
        let page = self
            .pages
            .storage
            .get(id)
            .ok_or(anyhow::anyhow!("Couldn't get page"))?;
        let parents_before: HashSet<BlobID> = page.get_parents().into_iter().collect();
        let cond = self
            .dht_storage
            .convert(page.cond(), &page.realm_id())
            .await;
        let new_page = page.edit_data_signers(cond, edit, signers)?;
        self.store_page(new_page.clone())?;

        // Make sure the parent.children links get correctly updated.
        let parents_after: HashSet<BlobID> = new_page.get_parents().into_iter().collect();
        self.update_child_links(
            parents_before.clone(),
            parents_after.clone(),
            |pb| pb.rm_child(id),
            signers,
        )
        .await;
        self.update_child_links(
            parents_after,
            parents_before,
            |pb| {
                pb.rm_child(id);
                pb.add_child(id.clone())
            },
            signers,
        )
        .await;
        Ok(new_page)
    }

    async fn update_child_links(
        &mut self,
        before: HashSet<BlobID>,
        after: HashSet<BlobID>,
        mut edit: impl FnMut(&mut BlobPage),
        signers: &[&Signer],
    ) {
        for pid in before.difference(&after) {
            if let Some(parent) = self.pages.storage.get(pid) {
                let cond = self
                    .dht_storage
                    .convert(parent.cond(), &parent.realm_id())
                    .await;
                if cond.can_signers(signers) {
                    match parent.edit_data_signers(cond, &mut edit, signers) {
                        Ok(new_parent) => {
                            self.store_page(new_parent.clone())
                                .expect("Storing new parent");
                        }
                        Err(e) => log::warn!("Couldn't update parent: {e:?}"),
                    }
                }
            }
        }
    }

    pub fn store_page(&mut self, page: FloBlobPage) -> anyhow::Result<()> {
        self.dht_storage.store_flo(page.clone().into())?;
        self.pages.storage.insert(page.blob_id(), page);
        Ok(())
    }

    pub fn store_tag(&mut self, tag: FloBlobTag) -> anyhow::Result<()> {
        self.dht_storage.store_flo(tag.clone().into())?;
        self.tags.storage.insert(tag.blob_id(), tag);
        Ok(())
    }

    pub async fn get_tap_out(
        &mut self,
    ) -> anyhow::Result<(UnboundedReceiver<DHTStorageOut>, usize)> {
        Ok(self.dht_storage.broker.get_tap_out().await?)
    }

    pub async fn get_cond<T: Serialize + DeserializeOwned + Clone + std::fmt::Debug>(
        &mut self,
        flo: &FloWrapper<T>,
    ) -> Condition {
        self.dht_storage.convert(flo.cond(), &flo.realm_id()).await
    }

    pub async fn update_realm(&mut self) -> anyhow::Result<&FloRealm> {
        self.realm = self.dht_storage.get_flo(&self.realm.global_id()).await?;
        Ok(&self.realm)
    }

    pub async fn update_all(&mut self) -> anyhow::Result<()> {
        self.update_pages().await?;
        self.update_tags().await?;
        self.update_realm().await?;

        Ok(())
    }
}

impl<T: Serialize + DeserializeOwned + Clone + std::fmt::Debug> RealmStorage<T>
where
    FloWrapper<T>: BlobPath + BlobFamily,
{
    async fn from_root(dht_storage: DHTStorage, root: GlobalID) -> anyhow::Result<Self> {
        let mut rs = Self {
            realm: root.realm_id().clone(),
            root: (*root.flo_id().clone()).into(),
            storage: HashMap::new(),
            cuckoos: HashMap::new(),
            ds: dht_storage,
        };
        rs.update_cuckoos(&rs.root.clone()).await?;
        Ok(rs)
    }

    async fn from_service(
        ds: DHTStorage,
        fr: &FloRealm,
        service: &str,
    ) -> anyhow::Result<Option<Self>>
    where
        FloWrapper<T>: BlobPath + BlobFamily,
    {
        Ok(if let Some(id) = fr.cache().get_services().get(service) {
            Some(RealmStorage::from_root(ds, fr.realm_id().global_id(id.clone())).await?)
        } else {
            None
        })
    }

    pub fn get_path(&self, path: &str) -> anyhow::Result<&FloWrapper<T>> {
        self.get_path_internal(
            path.trim_start_matches("/").split('/').collect::<Vec<_>>(),
            vec![&self.root],
        )
    }

    /// Returns the Blob and its cuckoos.
    /// If no blob with this ID is stored, it starts by fetching this blob and its cuckoos.
    /// If the blob doesn't exist, but its cuckoos, then it will try to fetch the blob, and then
    /// return either the blob and its cuckoos, or just the cuckoos.
    pub async fn get_cuckoos(&mut self, bid: BlobID) -> anyhow::Result<Vec<&FloWrapper<T>>> {
        // if self.storage.get(&bid).is_none() {
        self.update_cuckoos(&bid).await?;
        // }
        Ok(once(&bid)
            .chain(self.cuckoos.get(&bid).unwrap_or(&Vec::new()))
            .filter_map(|id| self.storage.get(id))
            .collect::<Vec<_>>())
    }

    fn get_path_internal(
        &self,
        parts: Vec<&str>,
        curr: Vec<&BlobID>,
    ) -> anyhow::Result<&FloWrapper<T>> {
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
                    return Ok(f);
                } else {
                    let children = f.get_children();
                    if !children.is_empty() {
                        return self.get_path_internal(part.1.to_vec(), children.iter().collect());
                    }
                }
            }
        }
        Err(anyhow::anyhow!("Couldn't find page"))
    }

    async fn update_cuckoos(&mut self, bid: &BlobID) -> anyhow::Result<()> {
        for id in self
            .ds
            .get_cuckoos(&self.realm.global_id((**bid).into()))
            .await
            .unwrap_or_default()
            .into_iter()
            .chain(std::iter::once((**bid).into()))
        {
            if let Ok(flo) = self
                .ds
                .get_flo::<T>(&self.realm.global_id(id.clone()))
                .await
            {
                self.storage.insert((*flo.flo_id()).into(), flo.clone());
                let blob_id: BlobID = (*flo.flo_id()).into();
                if *bid != blob_id {
                    let cuckoos = self.cuckoos.entry(bid.clone()).or_insert_with(Vec::new);
                    if !cuckoos.contains(&blob_id) {
                        cuckoos.push(blob_id);
                    }
                }
            }
        }
        Ok(())
    }

    async fn update_blobs(&mut self) -> anyhow::Result<()> {
        let bids = self.storage.keys().cloned().collect::<Vec<_>>();
        for bid in bids {
            self.update_cuckoos(&bid).await?;
        }
        Ok(())
    }
}

pub struct RealmViewBuilder {
    dht_storage: DHTStorage,
    name: String,
    cond: Condition,
    signers: Vec<Signer>,
    config: RealmConfig,
    page: Option<RVBuilderBlob>,
    tag: Option<RVBuilderBlob>,
}

struct RVBuilderBlob {
    path_name: String,
    content: Option<String>,
    parent: Option<BlobID>,
    cond: Condition,
    cuckoo: Cuckoo,
    signers: Vec<Signer>,
}

impl RealmViewBuilder {
    pub fn new(
        dht_storage: DHTStorage,
        name: String,
        cond: Condition,
        signers: Vec<Signer>,
    ) -> Self {
        Self {
            dht_storage,
            name,
            cond,
            config: RealmConfig {
                max_space: 1e6 as u64,
                max_flo_size: 1e4 as u32,
            },
            signers,
            page: None,
            tag: None,
        }
    }

    pub fn config(mut self, config: RealmConfig) -> Self {
        self.config = config;
        self
    }

    pub fn root_http(
        self,
        path: String,
        content: String,
        parent: Option<BlobID>,
        cond: Condition,
        signers: Vec<Signer>,
    ) -> Self {
        Self::root_http_cuckoo(self, path, content, parent, cond, Cuckoo::None, signers)
    }

    pub fn root_http_cuckoo(
        mut self,
        path: String,
        content: String,
        parent: Option<BlobID>,
        cond: Condition,
        cuckoo: Cuckoo,
        signers: Vec<Signer>,
    ) -> Self {
        self.page = Some(RVBuilderBlob {
            path_name: path,
            content: Some(content),
            parent,
            cond,
            cuckoo,
            signers,
        });
        self
    }

    pub fn root_tag(
        self,
        name: String,
        parent: Option<BlobID>,
        cond: Condition,
        signers: Vec<Signer>,
    ) -> Self {
        Self::root_tag_cuckoo(self, name, parent, cond, Cuckoo::None, signers)
    }

    pub fn root_tag_cuckoo(
        mut self,
        name: String,
        parent: Option<BlobID>,
        cond: Condition,
        cuckoo: Cuckoo,
        signers: Vec<Signer>,
    ) -> Self {
        self.tag = Some(RVBuilderBlob {
            path_name: name,
            content: None,
            parent,
            cond,
            cuckoo,
            signers,
        });
        self
    }

    pub async fn build(mut self) -> anyhow::Result<RealmView> {
        let p = self.page.ok_or(anyhow::anyhow!("Need root page"))?;
        let t = self.tag.ok_or(anyhow::anyhow!("Need root tag"))?;
        let realm_signers = &self.signers.iter().collect::<Vec<&Signer>>();
        let realm = FloRealm::new(&self.name, self.cond.clone(), self.config, realm_signers)?;
        self.dht_storage.store_flo(realm.clone().into())?;

        let ft = FloBlobTag::new_cuckoo(
            realm.realm_id(),
            t.cond,
            &t.path_name,
            t.parent,
            t.cuckoo,
            &t.signers.iter().collect::<Vec<&Signer>>(),
        )?;
        self.dht_storage.store_flo(ft.clone().into())?;

        let fp = FloBlobPage::new_cuckoo(
            realm.realm_id(),
            p.cond,
            &p.path_name,
            Bytes::from(p.content.unwrap()),
            p.parent.clone(),
            p.cuckoo,
            &p.signers.iter().collect::<Vec<&Signer>>(),
        )?;
        self.dht_storage.store_flo(fp.clone().into())?;

        let realm = realm.edit_data_signers(
            self.cond,
            |r| {
                r.set_service("http", fp.flo_id());
                r.set_service("tag", ft.flo_id());
            },
            &realm_signers,
        )?;
        self.dht_storage.store_flo(realm.clone().into())?;

        RealmView::new_from_realm(self.dht_storage, realm).await
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use flarch::{broker::Broker, data_storage::DataStorageTemp, nodeids::NodeID};

    use crate::{
        dht_storage::{broker::DHTStorage, core::DHTConfig},
        timer::Timer,
    };

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

    #[tokio::test]
    async fn get_path() -> anyhow::Result<()> {
        let ds = DHTStorage::start(
            DataStorageTemp::new_box(),
            DHTConfig::default(NodeID::rnd()),
            Timer::start().await?.broker,
            Broker::new(),
        )
        .await?;
        let mut rs: RealmStorage<BlobPage> = RealmStorage {
            realm: RealmID::from_str("00")?,
            root: BlobID::from_str("00")?,
            storage: HashMap::new(),
            cuckoos: HashMap::new(),
            ds: DHTStorage::from_broker(ds.broker, 1000).await?,
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

        assert!(rs.get_path("/").is_err());
        assert!(rs.get_path("blob").is_err());
        // Cannot compare root itself, as the rs.root got some children...
        assert!(rs
            .get_path("danu")
            .map(|f| f.blob_id())
            .is_ok_and(|id| id == root.blob_id()));
        assert!(rs
            .get_path("dahu")
            .map(|f| f.blob_id())
            .is_ok_and(|id| id == root_c0.blob_id()));
        assert!(rs
            .get_path("danu/blog")
            .map(|f| f.blob_id())
            .is_ok_and(|id| id == root_blog.blob_id()));
        assert!(rs
            .get_path("dahu/montagne")
            .map(|f| f.blob_id())
            .is_ok_and(|id| id == root_c0_article.blob_id()));

        Ok(())
    }
}
