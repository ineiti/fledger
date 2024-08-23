use serde::{Deserialize, Serialize};

/// Whatever hardcoded config you want to pass to your module.
/// It must have a default option.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TemplateConfig {
    multiplier: u32,
}

impl Default for TemplateConfig {
    fn default() -> Self {
        Self { multiplier: 1 }
    }
}

/// The TemplateCore structure holds a configuration and the storage
/// needed to persist over reloads of the node.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TemplateCore {
    pub storage: TemplateStorage,
    pub config: TemplateConfig,
}

impl TemplateCore {
    /// Initializes an EventsStorage with two categories.
    pub fn new(storage: TemplateStorage, config: TemplateConfig) -> Self {
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
pub enum TemplateStorageSave {
    V1(TemplateStorage),
}

impl TemplateStorageSave {
    pub fn from_str(data: &str) -> Result<TemplateStorage, serde_yaml::Error> {
        return Ok(serde_yaml::from_str::<TemplateStorageSave>(data)?.to_latest());
    }

    fn to_latest(self) -> TemplateStorage {
        match self {
            TemplateStorageSave::V1(es) => es,
        }
    }
}

/// If you want to add a new version and the current version is `x`, do the
/// following:
/// - copy `TemplateStorage` to a struct called `TemplateStorageVx`
/// - change the `TemplateStorage` to include your new fields
/// - change the `TemplateStorageSave` to include the new version:
///   - add a `Vx` to the name of the structure of the `Vx` enum
///   - add a `V(x+1)` enum pointing to `TemplateStorage`
/// - adapt `TemplateStorageSave::to_latest` to go from `Vx` to `V(x+1)`
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TemplateStorage {
    pub counter: u32,
}

impl TemplateStorage {
    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string::<TemplateStorageSave>(&TemplateStorageSave::V1(self.clone()))
    }
}

impl Default for TemplateStorage {
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
        let mut tc = TemplateCore::new(TemplateStorage::default(), TemplateConfig::default());
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
