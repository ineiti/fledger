use flarch::nodeids::{NodeIDs, U256};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct LedgerSetup {
    pub id: U256,
    pub spread: u32,
    pub min_mem: u64,
    pub dynamic: bool,
    pub members: NodeIDs,
}

pub enum LedgerStatus {
    Empty,
    Connected,
    Live,
}

/// Whatever hardcoded config you want to pass to your module.
/// It must have a default option.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct LedgerConfig {}

impl Default for LedgerConfig {
    fn default() -> Self {
        Self {}
    }
}

/// The TemplateCore structure holds a configuration and the storage
/// needed to persist over reloads of the node.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Ledger {
    pub storage: LedgerStorage,
    pub config: LedgerConfig,
}

impl Ledger {
    /// Initializes a new TemplateCore.
    pub fn new(storage: LedgerStorage, config: LedgerConfig) -> Self {
        Self { storage, config }
    }
}

/// The storage will probably evolve over time, so it's a good idea to store the different
/// versions in an enum.
/// This allows to update to the latest version, supposing that new fields can be filled
/// with default values.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum LedgerStorageSave {
    V1(LedgerStorage),
}

impl LedgerStorageSave {
    pub fn from_str(data: &str) -> Result<LedgerStorage, serde_yaml::Error> {
        return Ok(serde_yaml::from_str::<LedgerStorageSave>(data)?.to_latest());
    }

    fn to_latest(self) -> LedgerStorage {
        match self {
            LedgerStorageSave::V1(es) => es,
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
pub struct LedgerStorage {
    pub counter: u32,
}

impl LedgerStorage {
    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string::<LedgerStorageSave>(&LedgerStorageSave::V1(self.clone()))
    }
}

impl Default for LedgerStorage {
    fn default() -> Self {
        Self { counter: 0 }
    }
}

/// Here you must write the necessary unit-tests to make sure that your core algorithms
/// work the way you want them to.
#[cfg(test)]
mod tests {
    use std::error::Error;

    // use super::*;

    #[test]
    fn test_increase() -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}
