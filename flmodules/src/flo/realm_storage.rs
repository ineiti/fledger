use std::{collections::HashMap, fmt::Debug, iter::once};

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};

use crate::{
    dht_storage::broker::DHTStorage,
    flo::{
        blob::{BlobFamily, BlobID, BlobPath},
        flo::{FloID, FloWrapper},
        realm::{FloRealm, GlobalID, RealmID},
    },
};

#[async_trait(?Send)]
pub trait DHTFetcher<T: Serialize + DeserializeOwned + Clone>: std::fmt::Debug {
    async fn get_cuckoos(&mut self, id: &GlobalID) -> anyhow::Result<Vec<FloID>>;
    async fn get_flo(&mut self, id: &GlobalID) -> anyhow::Result<FloWrapper<T>>;
}

#[derive(Debug)]
pub struct RealmStorage<T> {
    pub realm: RealmID,
    pub ds: Box<dyn DHTFetcher<T>>,
    pub root: BlobID,
    pub storage: HashMap<BlobID, FloWrapper<T>>,
    pub cuckoos: HashMap<BlobID, Vec<BlobID>>,
}

impl<T: Serialize + DeserializeOwned + Clone + std::fmt::Debug> RealmStorage<T>
where
    FloWrapper<T>: BlobPath + BlobFamily,
{
    pub async fn from_root(ds: Box<dyn DHTFetcher<T>>, root: GlobalID) -> anyhow::Result<Self> {
        let mut rs = Self {
            realm: root.realm_id().clone(),
            root: (*root.flo_id().clone()).into(),
            storage: HashMap::new(),
            cuckoos: HashMap::new(),
            ds,
        };
        rs.update_cuckoos(&rs.root.clone()).await?;
        Ok(rs)
    }

    pub async fn from_service(
        ds: Box<dyn DHTFetcher<T>>,
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

    pub fn get_path_internal(
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

    pub async fn update_cuckoos(&mut self, bid: &BlobID) -> anyhow::Result<()> {
        for id in self
            .ds
            .get_cuckoos(&self.realm.global_id((**bid).into()))
            .await
            .unwrap_or_default()
            .into_iter()
            .chain(std::iter::once((**bid).into()))
        {
            if let Ok(flo) = self.ds.get_flo(&self.realm.global_id(id.clone())).await {
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

    pub async fn update_blobs(&mut self) -> anyhow::Result<()> {
        let bids = self.storage.keys().cloned().collect::<Vec<_>>();
        for bid in bids {
            self.update_cuckoos(&bid).await?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct DHTFetcherStorage {
    pub ds: DHTStorage,
}

impl<T: Serialize + DeserializeOwned + Clone> From<DHTStorage> for Box<dyn DHTFetcher<T>> {
    fn from(ds: DHTStorage) -> Self {
        Box::new(DHTFetcherStorage { ds })
    }
}

#[async_trait(?Send)]
impl<T: Serialize + DeserializeOwned + Clone> DHTFetcher<T> for DHTFetcherStorage {
    async fn get_cuckoos(&mut self, id: &GlobalID) -> anyhow::Result<Vec<FloID>> {
        self.ds.get_cuckoos(id).await
    }

    async fn get_flo(&mut self, id: &GlobalID) -> anyhow::Result<FloWrapper<T>> {
        self.ds.get_flo(id).await
    }
}
