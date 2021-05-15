use js_sys::Date;
use log::{error, info};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsValue;
use wasm_webrtc::{web_rtc_setup::WebRTCConnectionSetupWasm, web_socket::WebSocketWasm};
use wasm_webrtc::helpers::wait_ms;

use common::{
    node::{logic::stats::StatNode, version::VERSION_STRING, Node},
    types::DataStorage,
};

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

struct DummyDS {}

impl DataStorage for DummyDS {
    fn load(&self, _key: &str) -> Result<String, String> {
        fsread(STORAGE_NAME).map_err(|e| format!("While reading file: {:?}", e))
    }

    fn save(&self, _key: &str, value: &str) -> Result<(), String> {
        fswrite(STORAGE_NAME, value);
        Ok(())
    }
}

async fn start(url: &str) -> Result<Node, JsValue> {
    let rtc_spawner = Box::new(|cs| WebRTCConnectionSetupWasm::new(cs));
    let my_storage = Box::new(DummyDS {});
    let ws = WebSocketWasm::new(url)?;
    Node::new(my_storage, "node",  Box::new(ws), rtc_spawner).map_err(|e| JsValue::from(e))
}

async fn list_ping(n: &mut Node) -> Result<(), String> {
    n.list()?;
    n.ping().await?;
    let mut nodes: Vec<StatNode> = n.stats()?.iter().map(|(_k, v)| v.clone()).collect();
    nodes.sort_by(|a, b| b.last_contact.partial_cmp(&a.last_contact).unwrap());
    for node in nodes {
        if let Some(info) = node.node_info.as_ref() {
            if n.info()?.id != info.id {
                info!(
                    "Node: name:{} age:{} ping:({}/{}) conn:({:?}/{:?})",
                    info.info,
                    ((Date::now() - node.last_contact) / 1000.).floor(),
                    node.ping_rx,
                    node.ping_tx,
                    node.incoming,
                    node.outgoing,
                );
            }
        }
    }
    Ok(())
}

#[wasm_bindgen(start)]
pub async fn run_app() {
    console_error_panic_hook::set_once();

    wasm_logger::init(wasm_logger::Config::default());

    info!("Starting app with version {}", VERSION_STRING);

    let mut node = match start(URL).await {
        Ok(node) => node,
        Err(e) => {
            error!("Error while creating node: {:?}", e);
            return;
        }
    };
    info!("Started successfully");
    let mut i: i32 = 0;
    loop {
        i = i + 1;
        if let Err(e) = node.process().await {
            error!("Error while processing messages: {}", e);
        }

        if i % 3 == 2 {
            info!("Waiting");
            if let Err(e) = list_ping(&mut node).await {
                error!("Couldn't list or ping nodes: {}", e);
            }
        }
        wait_ms(1000).await;
    }
}
