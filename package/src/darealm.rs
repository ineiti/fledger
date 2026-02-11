use flmodules::{
    flo::{blob::BlobAccess, flo, realm, realm_view::RealmView},
};
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct RealmID(realm::RealmID);

impl RealmID {
    pub fn new(id: realm::RealmID) -> RealmID {
        RealmID(id)
    }

    pub fn get_id(&self) -> realm::RealmID {
        self.0.clone()
    }
}

#[wasm_bindgen]
impl RealmID {
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string(&self) -> String {
        format!("{}", self.0)
    }
}

#[wasm_bindgen]
pub struct FloID(flo::FloID);

impl FloID {
    pub fn new(id: flo::FloID) -> FloID {
        FloID(id)
    }

    pub fn get_id(&self) -> flo::FloID {
        self.0.clone()
    }
}

#[wasm_bindgen]
impl FloID {
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string(&self) -> String {
        format!("{}", self.0)
    }
}

#[wasm_bindgen]
#[derive(Debug)]
pub struct DaRealm {
    realm: RealmView,
}

impl DaRealm {
    pub fn new(r: RealmView) -> DaRealm {
        DaRealm { realm: r }
    }
}

#[wasm_bindgen]
impl DaRealm {
    pub fn get_path(&mut self, path: &str) -> Result<String, String> {
        String::from_utf8(self.get_path_data(path, "index.html")?).map_err(|e| format!("{e}"))
    }

    pub fn get_path_data(&mut self, path: &str, data: &str) -> Result<Vec<u8>, String> {
        Ok(self
            .realm
            .get_page_from_path(path)
            .map_err(|e| format!("{e}"))?
            .get_data(data)
            .ok_or(format!("Data {data} doesn't exist"))?
            .to_vec())
    }
}
