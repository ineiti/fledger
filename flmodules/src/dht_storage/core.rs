use std::collections::HashMap;

use flarch::nodeids::U256;
use serde::{Deserialize, Serialize};

use crate::flo::dht::{DHTFlo, DHTStorageConfig};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct FloMeta {
    pub id: U256,
    pub size: u64,
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

/// If you want to add a new version and the current version is `x`, do the
/// following:
/// - copy `DHTStorageStorage` to a struct called `DHTStorageStorageVx`
/// - change the `DHTStorageStorage` to include your new fields
/// - change the `DHTStorageStorageSave` to include the new version:
///   - add a `Vx` to the name of the structure of the `Vx` enum
///   - add a `V(x+1)` enum pointing to `DHTStorageStorage`
/// - adapt `DHTStorageStorageSave::to_latest` to go from `Vx` to `V(x+1)`
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct DHTStorageBucket {
    pub flos: HashMap<U256, DHTFlo>,
}

impl DHTStorageBucket {
    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string::<DHTStorageStorageSave>(&DHTStorageStorageSave::V1(self.clone()))
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
