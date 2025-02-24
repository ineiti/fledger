use std::{
    fs::{create_dir_all, remove_file, File},
    path::PathBuf,
};

use async_trait::async_trait;

use crate::data_storage::{DataStorage, StorageError};

pub struct DataStorageLocal {}
impl DataStorageLocal {
    pub fn new(_: &str) -> Box<dyn DataStorage + Send> {
        panic!("Only implemented for target_family=wasm");
    }

    pub fn clone() -> Box<dyn DataStorage + Send> {
        panic!("Only implemented for target_family=wasm");
    }
}

pub struct DataStorageFile {
    dir: PathBuf,
    base: String,
}

impl DataStorageFile {
    pub fn new(root: String, base: String) -> Self {
        let mut dir = PathBuf::new();
        dir.push(root);
        if !dir.exists() {
            create_dir_all(dir.clone()).expect("Creating directory");
        }
        Self { dir, base }
    }

    fn name(&self, key: &str) -> PathBuf {
        let mut name = self.dir.clone();
        name.push(if self.base.is_empty() {
            format!("fledger_{}.toml", key)
        } else {
            format!("{}_{}.toml", self.base, key)
        });
        name
    }
}

use std::io::prelude::*;

#[async_trait]
impl DataStorage for DataStorageFile {
    fn get(&self, key: &str) -> Result<String, StorageError> {
        let name = &self.name(key);
        Ok(if name.exists() {
            let mut contents = String::new();
            File::open(name)
                .map_err(|e| StorageError::Underlying(format!("While opening file: {:?}", e)))?
                .read_to_string(&mut contents)
                .map_err(|e| StorageError::Underlying(format!("While reading file: {:?}", e)))?;
            contents
        } else {
            "".into()
        })
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), StorageError> {
        File::create(self.name(key))
            .map_err(|e| StorageError::Underlying(format!("While creating file: {:?}", e)))?
            .write_all(value.as_bytes())
            .map_err(|e| StorageError::Underlying(format!("While writing file: {:?}", e)))?;
        Ok(())
    }

    fn remove(&mut self, key: &str) -> Result<(), StorageError> {
        remove_file(self.name(key))
            .map_err(|e| StorageError::Underlying(format!("While removing file: {:?}", e)))?;
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn DataStorage + Send> {
        Box::new(DataStorageFile {
            dir: self.dir.clone(),
            base: self.base.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    #[test]
    fn write_read() -> Result<(), Box<dyn Error>> {
        let mut storage = DataStorageFile::new("/tmp/test".into(), "one".into());
        storage.set("two", "three")?;
        assert_eq!("three", storage.get("two")?);

        let storage = DataStorageFile::new("/tmp/test".into(), "one".into());
        assert_eq!("three", storage.get("two")?);
        Ok(())
    }
}
