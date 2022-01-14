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
}
