use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("From the underlying storage: {0}")]
    Underlying(String),
}

/// A DataStorageBase can give different DataStorages.
/// Each DataStorage must present an independant namespace for `get`
/// and `put`.
pub trait DataStorageBase {
    fn get(&self, base: &str) -> Box<dyn DataStorage>;

    fn clone(&self) -> Box<dyn DataStorageBase>;
}

/// The DataStorage trait allows access to a persistent storage. Each module
/// has it's own DataStorage, so there will never be a name clash.
pub trait DataStorage {
    fn get(&self, key: &str) -> Result<String, StorageError>;

    fn set(&mut self, key: &str, value: &str) -> Result<(), StorageError>;

    fn remove(&mut self, key: &str) -> Result<(), StorageError>;
}

/// A temporary DataStorageBase that hands out ephemeral, memory-base
/// DataStorages.
pub struct TempDSB {}

impl TempDSB {
    pub fn new() -> Box<Self> {
        Box::new(Self {})
    }
}

impl DataStorageBase for TempDSB {
    fn get(&self, _: &str) -> Box<dyn DataStorage> {
        TempDS::new()
    }
    fn clone(&self) -> Box<dyn DataStorageBase> {
        TempDSB::new()
    }
}

/// A temporary DataStorage.
pub struct TempDS {
    kvs: HashMap<String, String>,
}

impl TempDS {
    pub fn new() -> Box<Self> {
        Box::new(Self {
            kvs: HashMap::new(),
        })
    }
}

impl DataStorage for TempDS {
    fn get(&self, key: &str) -> Result<String, StorageError> {
        Ok(self.kvs.get(key).unwrap_or(&"".to_string()).clone())
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), StorageError> {
        self.kvs.insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn remove(&mut self, key: &str) -> Result<(), StorageError> {
        self.kvs.remove(key);
        Ok(())
    }
}
