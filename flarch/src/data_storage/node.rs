use crate::data_storage::{DataStorage, StorageError};
use wasm_bindgen::{prelude::*, JsValue};
use async_trait::async_trait;

#[wasm_bindgen(
    inline_js = "module.exports.fswrite = function(name, str) { fs.writeFileSync(name, str); }
    module.exports.fsread = function(name) { return fs.readFileSync(name, {encoding: 'utf-8'}); }
    module.exports.fsexists = function(name) { return fs.existsSync(name); }
    module.exports.fsunlink = function(name) { return fs.unlinkSync(name); }"
)]
extern "C" {
    pub fn fswrite(name: &str, str: &str);
    #[wasm_bindgen(catch)]
    pub fn fsread(name: &str) -> Result<String, JsValue>;
    pub fn fsexists(name: &str) -> bool;
    #[wasm_bindgen(catch)]
    pub fn fsunlink(name: &str) -> Result<(), JsValue>;
}

pub struct DataStorageNode {
    base: String,
}

impl DataStorageNode {
    pub fn new(base: String) -> Self {
        Self { base }
    }
    
    fn name(&self, key: &str) -> String {
        if self.base.is_empty() {
            format!("fledger_{}.yaml", key)
        } else {
            format!("{}_{}.yaml", self.base, key)
        }
    }
}

#[async_trait(?Send)]
impl DataStorage for DataStorageNode {
    fn get(&self, key: &str) -> Result<String, StorageError> {
        let name = &self.name(key);
        Ok(if fsexists(name) {
            fsread(name)
                .map_err(|e| StorageError::Underlying(format!("While reading file: {:?}", e)))?
        } else {
            "".into()
        })
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), StorageError> {
        fswrite(&self.name(key), value);
        Ok(())
    }

    fn remove(&mut self, key: &str) -> Result<(), StorageError> {
        let name = &self.name(key);
        if fsexists(name) {
            fsunlink(name)
                .map_err(|e| StorageError::Underlying(format!("While unlinking file: {:?}", e)))?
        };
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn DataStorage + Send> {
        Box::new(Self {
            base: self.base.clone(),
        })
    }
}
