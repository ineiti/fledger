use async_trait::async_trait;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use thiserror::Error;

#[cfg(all(target_family = "wasm", feature = "node"))]
mod node;
#[cfg(all(target_family = "wasm", feature = "node"))]
pub use node::*;
#[cfg(all(target_family = "wasm", not(feature = "node")))]
mod wasm;
#[cfg(all(target_family = "wasm", not(feature = "node")))]
pub use wasm::*;
#[cfg(target_family = "unix")]
mod libc;
#[cfg(target_family = "unix")]
pub use libc::*;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("From the underlying storage: {0}")]
    Underlying(String),
}

/// The DataStorage trait allows access to a persistent storage.
#[async_trait]
pub trait DataStorage {
    fn get(&self, key: &str) -> Result<String, StorageError>;

    fn set(&mut self, key: &str, value: &str) -> Result<(), StorageError>;

    fn remove(&mut self, key: &str) -> Result<(), StorageError>;

    fn clone_box(&self) -> Box<dyn DataStorage + Send>;
}

impl Clone for Box<dyn DataStorage + Send> {
    fn clone(&self) -> Box<dyn DataStorage + Send> {
        self.clone_box()
    }
}

impl std::fmt::Debug for Box<dyn DataStorage + Send>{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Box<DataStorage>").finish()
    }
}

/// A temporary DataStorage that keeps the data only during its lifetime.
pub struct DataStorageTemp {
    kvs: Arc<Mutex<HashMap<String, String>>>,
}

impl DataStorageTemp {
    pub fn new() -> Self {
        Self {
            kvs: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    pub fn new_box() -> Box<Self> {
        Box::new(Self {
            kvs: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}

impl DataStorage for DataStorageTemp {
    fn get(&self, key: &str) -> Result<String, StorageError> {
        let mut kvs = self
            .kvs
            .try_lock()
            .map_err(|e| StorageError::Underlying(e.to_string()))?;
        if let Some(kvs_entry) = kvs.get_mut(key) {
            Ok(kvs_entry.clone())
        } else {
            Ok("".to_string())
        }
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), StorageError> {
        let mut kvs = self
            .kvs
            .try_lock()
            .map_err(|e| StorageError::Underlying(e.to_string()))?;
        kvs.insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn remove(&mut self, key: &str) -> Result<(), StorageError> {
        let mut kvs = self
            .kvs
            .try_lock()
            .map_err(|e| StorageError::Underlying(e.to_string()))?;
        kvs.remove(key);
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn DataStorage + Send> {
        Box::new(Self {
            kvs: Arc::clone(&self.kvs),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage() -> anyhow::Result<()> {
        let mut ds = DataStorageTemp::new();
        ds.set("two", "three")?;

        let ds2 = ds.clone_box();
        assert_eq!("three", ds2.get("two")?);
        Ok(())
    }
}
