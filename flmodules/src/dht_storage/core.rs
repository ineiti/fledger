use std::collections::HashMap;

use flarch::nodeids::U256;
use serde::{Deserialize, Serialize};

use crate::flo::dht::{DHTFlo, DHTStorageConfig};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct FloMeta {
    pub id: U256,
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
    pub storage: DHTStorageBucket,
    pub config: DHTStorageConfig,
}

impl DHTStorageCore {
    /// Initializes a new DHTStorageCore.
    pub fn new(storage: DHTStorageBucket, config: DHTStorageConfig) -> Self {
        Self { storage, config }
    }

    /// TODO: decide which IDs need to be stored.
    pub fn update_available(&self, available: Vec<FloMeta>) -> Vec<U256> {
        available.iter().map(|fm| fm.id).collect()
    }

    pub fn update_request(&self, keys: Vec<U256>) -> Vec<DHTFlo> {
        keys.iter()
            .filter_map(|k| self.storage.flos.get(k))
            .cloned()
            .collect()
    }

    pub fn update_flos(&mut self, flos: Vec<DHTFlo>) {
        for flo in flos {
            if let Some(old) = self.storage.flos.get(&flo.id()) {
                if old.flo.version() < flo.flo.version() {
                    self.storage.flos.insert(old.flo.id, flo);
                }
            }
        }
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct DHTStorageBucket {
    pub flos: HashMap<U256, DHTFlo>,
}

impl DHTStorageBucket {
    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string::<DHTStorageStorageSave>(&DHTStorageStorageSave::V1(self.clone()))
    }

    pub fn store_kv(&mut self, flo: DHTFlo) {
        self.flos.insert(flo.id(), flo);
        // TODO: evict values which are furthest from us
    }

    pub fn flo_meta(&self) -> Vec<FloMeta> {
        self.flos
            .values()
            .map(|df| FloMeta {
                id: df.id(),
                version: df.flo.version(),
            })
            .collect()
    }
}

/// Here you must write the necessary unit-tests to make sure that your core algorithms
/// work the way you want them to.
#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    #[test]
    fn test_increase() -> Result<(), Box<dyn Error>> {
        let _tc = DHTStorageCore::new(DHTStorageBucket::default(), DHTStorageConfig::default());
        Ok(())
    }
}
