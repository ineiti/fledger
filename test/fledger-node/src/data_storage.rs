use flutils::data_storage::{DataStorage, DataStorageBase, StorageError};
use wasm_bindgen::{prelude::*, JsValue};

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

pub struct DummyDSB {}

impl DataStorageBase for DummyDSB {
    fn get(&self, base: &str) -> Box<dyn DataStorage> {
        Box::new(DummyDS {
            base: base.to_string(),
        })
    }
    fn clone(&self) -> Box<dyn DataStorageBase> {
        Box::new(Self {})
    }
}

pub struct DummyDS {
    base: String,
}

impl DummyDS {
    fn name(&self, key: &str) -> String {
        if self.base.is_empty() {
            format!("fledger_{}.toml", key)
        } else {
            format!("{}_{}.toml", self.base, key)
        }
    }
}

impl DataStorage for DummyDS {
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
}
