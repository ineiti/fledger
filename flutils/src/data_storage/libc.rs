use crate::data_storage::{DataStorage, DataStorageBase, StorageError};

pub struct DataStorageBaseImpl {}

impl DataStorageBase for DataStorageBaseImpl {
    fn get(&self, _: &str) -> Box<dyn DataStorage> {
        Box::new(DataStorageImpl {})
    }
    fn clone(&self) -> Box<dyn DataStorageBase> {
        Box::new(DataStorageBaseImpl {})
    }
}

pub struct DataStorageImpl {}

impl DataStorage for DataStorageImpl {
    fn get(&self, _: &str) -> Result<String, StorageError> {
        Ok("".to_string())
    }

    fn set(&mut self, _: &str, _: &str) -> Result<(), StorageError> {
        Ok(())
    }

    fn remove(&mut self, _: &str) -> Result<(), StorageError> {
        Ok(())
    }
}
