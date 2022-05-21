use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use thiserror::Error;

#[cfg(all(target_arch = "wasm32", feature = "node"))]
mod node;
#[cfg(all(target_arch = "wasm32", feature = "node"))]
pub use node::*;
#[cfg(all(target_arch = "wasm32", not(feature = "node")))]
mod wasm;
#[cfg(all(target_arch = "wasm32", not(feature = "node")))]
pub use wasm::*;
#[cfg(not(target_arch = "wasm32"))]
mod libc;
#[cfg(not(target_arch = "wasm32"))]
pub use libc::*;

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
#[derive(Debug)]
pub struct TempDSB {
    kvs_base: Arc<Mutex<HashMap<String, HashMap<String, String>>>>,
}

impl TempDSB {
    pub fn new() -> Box<Self> {
        Box::new(Self {
            kvs_base: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}

impl DataStorageBase for TempDSB {
    fn get(&self, entry: &str) -> Box<dyn DataStorage> {
        TempDS::new(Arc::clone(&self.kvs_base), entry.into())
    }
    fn clone(&self) -> Box<dyn DataStorageBase> {
        Box::new(TempDSB {
            kvs_base: Arc::clone(&self.kvs_base),
        })
    }
}

/// A temporary DataStorage.
pub struct TempDS {
    kvs: Arc<Mutex<HashMap<String, HashMap<String, String>>>>,
    entry: String,
}

impl TempDS {
    pub fn new(
        kvs: Arc<Mutex<HashMap<String, HashMap<String, String>>>>,
        entry: String,
    ) -> Box<Self> {
        {
            let mut kvs_lock = kvs.lock().unwrap();
            if !kvs_lock.contains_key(&entry){
                kvs_lock.insert(entry.clone(), HashMap::new());
            }
        }
        Box::new(Self { kvs, entry })
    }
}

impl DataStorage for TempDS {
    fn get(&self, key: &str) -> Result<String, StorageError> {
        let mut kvs = self
            .kvs
            .try_lock()
            .map_err(|e| StorageError::Underlying(e.to_string()))?;
        if let Some(kvs_entry) = kvs.get_mut(&self.entry) {
            Ok(kvs_entry.get(key).unwrap_or(&"".to_string()).clone())
        } else {
            Ok("".to_string())
        }
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), StorageError> {
        let mut kvs = self
            .kvs
            .try_lock()
            .map_err(|e| StorageError::Underlying(e.to_string()))?;
        kvs.get_mut(&self.entry).unwrap().insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn remove(&mut self, key: &str) -> Result<(), StorageError> {
        let mut kvs = self
            .kvs
            .try_lock()
            .map_err(|e| StorageError::Underlying(e.to_string()))?;
        kvs.get_mut(&self.entry).unwrap().remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage() -> Result<(), Box<dyn std::error::Error>>{
        let dsb = TempDSB::new();
        let mut ds = dsb.get("one");
        ds.set("two", "three")?;

        let dsb2 = dsb.clone();
        let ds2 = dsb2.get("one");
        assert_eq!("three", ds2.get("two")?);
        Ok(())
    }
}