use common::node::ext_interface::{DataStorage,Logger};

use common::node::Node;
use wasm_bindgen::JsValue;
use web_sys::window;

use crate::{web_rtc_setup::WebRTCConnectionSetupWasm, web_socket::WebSocketWasm};

pub struct MyDataStorage {}

impl DataStorage for MyDataStorage {
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

pub struct WasmLogger {}

impl Logger for WasmLogger {
    fn info(&self, s: &str) {
        console_log!("info: {}", s);
    }

    fn warn(&self, s: &str) {
        console_warn!("warn: {}", s);
    }

    fn error(&self, s: &str) {
        console_warn!(" err: {}", s);
    }

    fn clone(&self) -> Box<dyn Logger> {
        Box::new(WasmLogger {})
    }
}

pub async fn start(log: Box<dyn Logger>, url: &str) -> Result<Node, JsValue> {
    let rtc_spawner = Box::new(|cs| WebRTCConnectionSetupWasm::new(cs));
    let my_storage = Box::new(MyDataStorage {});
    let ws = WebSocketWasm::new(url)?;
    let node = Node::new(my_storage, log, Box::new(ws), rtc_spawner)?;

    Ok(node)
}
