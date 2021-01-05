use common::ext_interface::DataStorage;
use common::ext_interface::Logger;

use common::ext_interface::WebRTCCallerState;
use common::node::Node;
use wasm_bindgen::JsValue;
use web_sys::window;

use crate::rest::RestCallerWasm;
use crate::web_rtc::WebRTCCallerWasm;
struct MyDataStorage {}

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

pub async fn start(log: Box<dyn Logger>) -> Result<Node, JsValue> {
    let rest_caller = RestCallerWasm::new("http://localhost:8000");
    let rtc_caller = WebRTCCallerWasm::new(WebRTCCallerState::Initializer)?;
    let my_storage = Box::new(MyDataStorage {});
    let node = Node::new(Box::new(rest_caller), Box::new(rtc_caller), my_storage, log)?;

    Ok(node)
}
