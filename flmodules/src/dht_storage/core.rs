use std::collections::HashMap;

use flarch::nodeids::U256;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::{
    dht_routing::kademlia::KNode,
    flo::{
        dht::{DHTFlo, DHTStorageConfig},
        flo::FloID,
    },
};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct FloMeta {
    pub id: FloID,
    pub version: usize,
}

impl Default for DHTStorageConfig {
    fn default() -> Self {
        Self {
            over_provide: 1.,
            max_space: 100_000,
        }
    }
}

/// The DHTStorageCore structure holds a configuration and the storage
/// needed to persist over reloads of the node.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DHTStorageCore {
    storage: DHTStorageBucket,
    config: DHTStorageConfig,
}

impl DHTStorageCore {
    /// Initializes a new DHTStorageCore.
    pub fn new(storage: DHTStorageBucket, config: DHTStorageConfig) -> Self {
        Self { storage, config }
    }

    /// TODO: decide which IDs need to be stored.
    pub fn update_available(&self, available: Vec<FloMeta>) -> Vec<FloID> {
        available.iter().map(|fm| fm.id.clone()).collect()
    }

    pub fn update_request(&self, keys: Vec<FloID>) -> Vec<DHTFlo> {
        keys.iter()
            .filter_map(|k| self.storage.flos.get(k))
            .cloned()
            .collect()
    }

    pub fn update_flos(&mut self, flos: Vec<DHTFlo>) {
        for flo in flos {
            if let Some(old) = self.storage.flos.get(&flo.id()) {
                if old.flo.version() < flo.flo.version() {
                    self.storage.flos.insert(old.flo.id.clone(), flo);
                }
            }
        }
    }

    pub fn store_kv(&mut self, flo: DHTFlo) {
        self.storage.put(flo);
        while self.storage.size() > self.config.max_space {
            self.storage.remove_furthest();
        }
        // TODO: evict values which are furthest from us
    }

    pub fn flo_meta(&self) -> Vec<FloMeta> {
        self.storage
            .flos
            .values()
            .map(|df| FloMeta {
                id: df.id(),
                version: df.flo.version(),
            })
            .collect()
    }

    pub fn get_flo(&self, key: &FloID) -> Option<&DHTFlo> {
        self.storage.flos.get(key)
    }

    pub fn storage_clone(&self) -> DHTStorageBucket {
        self.storage.clone()
    }
}

/// The storage will probably evolve over time, so it's a good idea to store the different
/// versions in an enum.
/// This allows to update to the latest version, supposing that new fields can be filled
/// with default values.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum DHTStorageStorageSave {
    V1(DHTStorageBucket),
}

impl DHTStorageStorageSave {
    pub fn from_str(data: &str) -> Result<DHTStorageBucket, serde_yaml::Error> {
        return Ok(serde_yaml::from_str::<DHTStorageStorageSave>(data)?.to_latest());
    }

    fn to_latest(self) -> DHTStorageBucket {
        match self {
            DHTStorageStorageSave::V1(es) => es,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DHTStorageBucket {
    root: U256,
    flos: HashMap<FloID, DHTFlo>,
    distances: HashMap<usize, Vec<FloID>>,
    size: usize,
}

impl DHTStorageBucket {
    pub fn new(root: U256) -> Self {
        Self {
            root,
            flos: HashMap::new(),
            distances: HashMap::new(),
            size: 0,
        }
    }

    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string::<DHTStorageStorageSave>(&DHTStorageStorageSave::V1(self.clone()))
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn put(&mut self, df: DHTFlo) {
        let id = df.id();
        self.remove_entry(&id);
        let depth = KNode::get_depth(&self.root, *id);
        self.distances
            .entry(depth)
            .or_insert_with(Vec::new)
            .push(id.clone());
        self.size += df.size();
        self.flos.insert(id, df);
    }

    pub fn get(&self, id: &FloID) -> Option<DHTFlo> {
        self.flos.get(id).cloned()
    }

    pub fn remove_furthest(&mut self) {
        if let Some(furthest) = self.distances.keys().sorted().unique().last() {
            if let Some(id) = self
                .distances
                .get(furthest)
                .and_then(|ids| ids.first())
                .cloned()
            {
                self.remove_entry(&id);
            }
        }
    }

    pub fn remove_entry(&mut self, id: &FloID) {
        if let Some(df) = self.flos.remove(id) {
            let distance = KNode::get_depth(&self.root, **id);
            self.distances
                .entry(distance)
                .and_modify(|v| v.retain(|i| i != id));
            self.size -= df.size();
        }
    }
}

/// Here you must write the necessary unit-tests to make sure that your core algorithms
/// work the way you want them to.
#[cfg(test)]
mod tests {
    use std::{error::Error, str::FromStr};

    use super::*;

    use crate::{crypto::access::Version, flo::testing::{new_ace, new_dht_flo_depth}};

    #[test]
    fn test_furthest() -> Result<(), Box<dyn Error>> {
        let root = U256::from_str("00").unwrap();
        let ace = new_ace()?;
        let ace_version = Version::Minimal(ace.get_id(), 0);
        let flos1: Vec<DHTFlo> = (1..=3)
            .map(|i| new_dht_flo_depth(ace_version.clone(), &root, i))
            .collect();
        let mut storage = DHTStorageBucket::new(root);
        for df in flos1 {
            storage.put(df);
        }

        assert_eq!(3, storage.distances.len());
        assert_eq!(3, storage.flos.len());
        let size = storage.size;

        let flos2: Vec<DHTFlo> = (1..=3)
            .map(|i| new_dht_flo_depth(ace_version.clone(), &root, i))
            .collect();
        for df in flos2 {
            storage.put(df);
        }
        assert_eq!(3, storage.distances.len());
        assert_eq!(2, storage.distances.get(&1).unwrap().len());
        assert_eq!(2, storage.distances.get(&2).unwrap().len());
        assert_eq!(2, storage.distances.get(&3).unwrap().len());
        assert_eq!(6, storage.flos.len());
        assert!(storage.size > size);
        let size = storage.size;

        storage.remove_furthest();
        assert_eq!(3, storage.distances.len());
        assert_eq!(2, storage.distances.get(&1).unwrap().len());
        assert_eq!(2, storage.distances.get(&2).unwrap().len());
        assert_eq!(1, storage.distances.get(&3).unwrap().len());
        assert_eq!(5, storage.flos.len());
        assert!(storage.size < size);
        let size = storage.size;

        storage.remove_furthest();
        assert_eq!(3, storage.distances.len());
        assert_eq!(2, storage.distances.get(&1).unwrap().len());
        assert_eq!(2, storage.distances.get(&2).unwrap().len());
        assert_eq!(0, storage.distances.get(&3).unwrap().len());
        assert_eq!(4, storage.flos.len());
        assert!(storage.size < size);

        Ok(())
    }
}
