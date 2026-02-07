use log::info;
use wasm_bindgen::prelude::*;

/// Initialize the WASM module with logging and panic hooks
#[wasm_bindgen]
pub fn initialize() {
    console_error_panic_hook::set_once();
    wasm_logger::init(wasm_logger::Config::default());
    info!("danode-core initialized");
}

pub mod danode;
pub mod darealm;
pub mod events;
