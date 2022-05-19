use std::{
    fs::{create_dir_all, remove_file, File},
    path::PathBuf,
};

use flutils::data_storage::{DataStorage, DataStorageBase, StorageError};

pub struct DataStorageFile {
    dir: PathBuf,
}

impl DataStorageFile {
    pub fn new(root: String) -> Self {
        let mut dir = PathBuf::new();
        dir.push(root);
        if !dir.exists() {
            create_dir_all(dir.clone())
                .expect("Creating directory");
        }
        Self { dir }
    }
}

impl DataStorageBase for DataStorageFile {
    fn get(&self, base: &str) -> Box<dyn DataStorage> {
        Box::new(DataStorageFileEntry {
            dir: self.dir.clone(),
            base: base.into(),
        })
    }

    fn clone(&self) -> Box<dyn DataStorageBase> {
        Box::new(DataStorageFile {
            dir: self.dir.clone(),
        })
    }
}

pub struct DataStorageFileEntry {
    dir: PathBuf,
    base: String,
}

impl DataStorageFileEntry {
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

impl DataStorage for DataStorageFileEntry {
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
}

#[cfg(test)]
mod tests{
    use std::error::Error;

    use super::*;

    #[test]
    fn write_read() -> Result<(), Box<dyn Error>>{
        let data = DataStorageFile::new("/tmp/test".into());
        let mut storage = data.get("one");
        storage.set("two", "three")?;
        assert_eq!("three", storage.get("two")?);

        let data = DataStorageFile::new("/tmp/test".into());
        assert_eq!("three", data.get("one").get("two")?);
        Ok(())
    }
}