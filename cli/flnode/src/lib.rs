use common::node::ext_interface::{DataStorage,Logger};

use common::node::Node;
use wasm_bindgen::JsValue;

use wasm_lib::{web_rtc_setup::WebRTCConnectionSetupWasm, web_socket::WebSocketWasm,
    logs::wait_ms};

use wasm_bindgen::prelude::*;

use wasm_lib::storage_logs::ConsoleLogger;

#[cfg(not(feature = "local"))]
const URL: &str = "wss://signal.fledg.re";

#[cfg(feature = "local")]
const URL: &str = "ws://localhost:8765";

#[wasm_bindgen(
    inline_js = "module.exports.fswrite = function(name, str) { fs.writeFileSync(name, str); }
    module.exports.fsread = function(name) { return fs.readFileSync(name, {encoding: 'utf-8'}); }"
)]
extern "C" {
    pub fn fswrite(name: &str, str: &str);
    #[wasm_bindgen(catch)]
    pub fn fsread(name: &str) -> Result<String, JsValue>;
}

const STORAGE_NAME: &str = "fledger.toml";

struct DummyDS{}

impl DataStorage for DummyDS {
    fn load(&self, _key: &str) -> Result<String, String> {
        fsread(STORAGE_NAME).map_err(|e| format!("While reading file: {:?}", e))
    }

    fn save(&self, _key: &str, value: &str) -> Result<(), String> {
        fswrite(STORAGE_NAME, value);
        Ok(())
    }
}

async fn start(log: Box<dyn Logger>, url: &str) -> Result<Node, JsValue> {
    let rtc_spawner = Box::new(|cs| WebRTCConnectionSetupWasm::new(cs));
    let my_storage = Box::new(DummyDS {});
    let ws = WebSocketWasm::new(url)?;
    let node = Node::new(my_storage, log, Box::new(ws), rtc_spawner)?;

    Ok(node)
}

async fn list_ping(n: &mut Node) -> Result<(), String>{
    n.list().await?;
    n.ping("something").await?;
    Ok(())
}

#[wasm_bindgen(start)]
pub async fn run_app() {
    console_error_panic_hook::set_once();

    let logger = ConsoleLogger{};
    logger.info("starting app for now!");

    let mut node = match start(logger.clone(), URL).await{
        Ok(node) => node,
        Err(e) => {
            logger.error(&format!("Error while creating node: {:?}", e));
            return;
        }
    };
    logger.info("Started successfully");
    loop{
        logger.info("Waiting");
        wait_ms(10000).await;
        if let Err(e) = list_ping(&mut node).await{
            logger.error(&format!("Couldn't list or ping nodes: {}", e));
        }
    }
}
