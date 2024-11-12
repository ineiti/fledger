use serde::{Deserialize, Serialize};

/// Whatever hardcoded config you want to pass to your module.
/// It must have a default option.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DHTRoutingConfig {
    multiplier: u32,
}

impl Default for DHTRoutingConfig {
    fn default() -> Self {
        Self { multiplier: 1 }
    }
}

/// The DHTRoutingCore structure holds a configuration and the storage
/// needed to persist over reloads of the node.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DHTRoutingCore {
    pub storage: DHTRoutingStorage,
    pub config: DHTRoutingConfig,
}

impl DHTRoutingCore {
    /// Initializes a new DHTRoutingCore.
    pub fn new(storage: DHTRoutingStorage, config: DHTRoutingConfig) -> Self {
        Self { storage, config }
    }

    // Here are the different methods to interact with this module.
    pub fn increase(&mut self, i: u32) {
        self.storage.counter += i * self.config.multiplier;
    }
}

/// The storage will probably evolve over time, so it's a good idea to store the different
/// versions in an enum.
/// This allows to update to the latest version, supposing that new fields can be filled
/// with default values.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum DHTRoutingStorageSave {
    V1(DHTRoutingStorage),
}

impl DHTRoutingStorageSave {
    pub fn from_str(data: &str) -> Result<DHTRoutingStorage, serde_yaml::Error> {
        return Ok(serde_yaml::from_str::<DHTRoutingStorageSave>(data)?.to_latest());
    }

    fn to_latest(self) -> DHTRoutingStorage {
        match self {
            DHTRoutingStorageSave::V1(es) => es,
        }
    }
}

/// If you want to add a new version and the current version is `x`, do the
/// following:
/// - copy `DHTRoutingStorage` to a struct called `DHTRoutingStorageVx`
/// - change the `DHTRoutingStorage` to include your new fields
/// - change the `DHTRoutingStorageSave` to include the new version:
///   - add a `Vx` to the name of the structure of the `Vx` enum
///   - add a `V(x+1)` enum pointing to `DHTRoutingStorage`
/// - adapt `DHTRoutingStorageSave::to_latest` to go from `Vx` to `V(x+1)`
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DHTRoutingStorage {
    pub counter: u32,
}

impl DHTRoutingStorage {
    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string::<DHTRoutingStorageSave>(&DHTRoutingStorageSave::V1(self.clone()))
    }
}

impl Default for DHTRoutingStorage {
    fn default() -> Self {
        Self { counter: 0 }
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
        let mut tc = DHTRoutingCore::new(DHTRoutingStorage::default(), DHTRoutingConfig::default());
        tc.increase(1);
        assert_eq!(1, tc.storage.counter);

        tc.increase(2);
        assert_eq!(3, tc.storage.counter);

        tc.config.multiplier = 2;
        tc.increase(1);
        assert_eq!(5, tc.storage.counter);
        
        Ok(())
    }
}
