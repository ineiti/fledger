use wasm_bindgen::prelude::*;
use web_sys::window;

use common::types::DataStorage;

pub struct LocalStorage {}

impl DataStorage for LocalStorage {
    fn load(&self, key: &str) -> Result<String, String> {
        window()
            .unwrap()
            .local_storage()
            .map_err(|e| e.as_string().unwrap())?
            .unwrap()
            .get(key)
            .map(|s| s.unwrap_or("".to_string()))
            .map_err(|e| e.as_string().unwrap())
    }

    fn save(&self, key: &str, value: &str) -> Result<(), String> {
        window()
            .unwrap()
            .local_storage()
            .map_err(|e| e.as_string().unwrap())?
            .unwrap()
            .set(key, value)
            .map_err(|e| e.as_string().unwrap())
    }
}

#[cfg_attr(
    feature = "node",
    wasm_bindgen(
        inline_js = "module.exports.wait_ms = function(ms){ return new Promise((r) => setTimeout(r, ms));}"
    )
)]
#[cfg_attr(
    not(feature = "node"),
    wasm_bindgen(
        inline_js = "export function wait_ms(ms){ return new Promise((r) => setTimeout(r, ms));}"
    )
)]
extern "C" {
    pub async fn wait_ms(ms: u32);
}
