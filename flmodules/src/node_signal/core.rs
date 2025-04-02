use serde::{Deserialize, Serialize};

/// Whatever hardcoded config you want to pass to your module.
/// It must have a default option.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct NodeSignalConfig {
    multiplier: u32,
}

impl Default for NodeSignalConfig {
    fn default() -> Self {
        Self { multiplier: 1 }
    }
}

/// The NodeSignalCore structure holds a configuration and the storage
/// needed to persist over reloads of the node.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct NodeSignalCore {
    pub storage: NodeSignalStorage,
}

impl NodeSignalCore {
    /// Initializes a new NodeSignalCore.
    pub fn new(storage: NodeSignalStorage) -> Self {
        Self { storage }
    }
}

/// The storage will probably evolve over time, so it's a good idea to store the different
/// versions in an enum.
/// This allows to update to the latest version, supposing that new fields can be filled
/// with default values.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum NodeSignalStorageSave {
    V1(NodeSignalStorage),
}

impl NodeSignalStorageSave {
    pub fn from_str(data: &str) -> anyhow::Result<NodeSignalStorage> {
        return Ok(serde_yaml::from_str::<NodeSignalStorageSave>(data)?.to_latest());
    }

    fn to_latest(self) -> NodeSignalStorage {
        match self {
            NodeSignalStorageSave::V1(es) => es,
        }
    }
}

/// If you want to add a new version and the current version is `x`, do the
/// following:
/// - copy `NodeSignalStorage` to a struct called `NodeSignalStorageVx`
/// - change the `NodeSignalStorage` to include your new fields
/// - change the `NodeSignalStorageSave` to include the new version:
///   - add a `Vx` to the name of the structure of the `Vx` enum
///   - add a `V(x+1)` enum pointing to `NodeSignalStorage`
/// - adapt `NodeSignalStorageSave::to_latest` to go from `Vx` to `V(x+1)`
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct NodeSignalStorage {
    pub counter: u32,
}

impl NodeSignalStorage {
    pub fn to_yaml(&self) -> anyhow::Result<String> {
        Ok(serde_yaml::to_string::<NodeSignalStorageSave>(
            &NodeSignalStorageSave::V1(self.clone()),
        )?)
    }
}

impl Default for NodeSignalStorage {
    fn default() -> Self {
        Self { counter: 0 }
    }
}

/// Here you must write the necessary unit-tests to make sure that your core algorithms
/// work the way you want them to.
#[cfg(test)]
mod tests {}
