use web_sys::window;

use crate::data_storage::{DataStorage, StorageError};

pub struct DataStorageLocal {
    base: String,
}

impl DataStorageLocal {
    pub fn new(base_str: &str) -> Box<dyn DataStorage> {
        let base = if base_str.is_empty() {
            "".to_string()
        } else {
            base_str.to_string() + "_"
        };
        Box::new(DataStorageLocal { base })
    }
}

impl DataStorage for DataStorageLocal {
    fn get(&self, key: &str) -> Result<String, StorageError> {
        let key_entry = format!("{}{}", self.base, key);
        Ok(window()
            .unwrap()
            .local_storage()
            .map_err(|e| StorageError::Underlying(e.as_string().unwrap()))?
            .unwrap()
            .get(&key_entry)
            .map_err(|e| StorageError::Underlying(e.as_string().unwrap()))?
            .unwrap_or_else(|| "".to_string()))
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), StorageError> {
        let key_entry = format!("{}{}", self.base, key);
        window()
            .unwrap()
            .local_storage()
            .map_err(|e| StorageError::Underlying(e.as_string().unwrap()))?
            .unwrap()
            .set(&key_entry, value)
            .map_err(|e| StorageError::Underlying(e.as_string().unwrap()))
    }

    fn remove(&mut self, key: &str) -> Result<(), StorageError> {
        let key_entry = format!("{}{}", self.base, key);
        window()
            .unwrap()
            .local_storage()
            .map_err(|e| StorageError::Underlying(e.as_string().unwrap()))?
            .unwrap()
            .remove_item(&key_entry)
            .map_err(|e| StorageError::Underlying(e.as_string().unwrap()))
    }

    fn clone(&self) -> Box<dyn DataStorage> {
        Box::new(DataStorageLocal {base: self.base.clone()})
    }
}
