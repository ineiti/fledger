//! Handels realms in an asynchronous way.
//! In Danu, realms are not guaranteed to be available right away, and updates
//! to the realms can come in at any time.
//! So while there is a local storage, there are no methods
//! to get a specific realm or a specific Flo from a realm,
//! only callbacks to listen to new realms.

use flmodules::flo::{blob::BlobAccess, realm_view::RealmView};
use wasm_bindgen::prelude::wasm_bindgen;

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
