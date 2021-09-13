use common::node::NodeError;
use common::signal::websocket::WSError;
use common::types::StorageError;
use js_sys::Date;
use log::{error, info, trace};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsValue;
use wasm_webrtc::{web_rtc_setup::WebRTCConnectionSetupWasm, web_socket::WebSocketWasm};
use wasm_webrtc::helpers::wait_ms;
use thiserror::Error;

use common::{
    node::{logic::stats::statnode::StatNode, version::VERSION_STRING, Node},
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
    fn load(&self, _key: &str) -> Result<String, StorageError> {
        Ok(fsread(STORAGE_NAME).map_err(|e| StorageError::Underlying(format!("While reading file: {:?}", e)))?)
    }

    fn save(&self, _key: &str, value: &str) -> Result<(), StorageError> {
        fswrite(STORAGE_NAME, value);
        Ok(())
    }
}

#[derive(Debug, Error)]
enum StartError{
    #[error(transparent)]
    Node(#[from]NodeError),
    #[error(transparent)]
    WS(#[from]WSError),
}

async fn start(url: &str) -> Result<Node, StartError> {
    let rtc_spawner = Box::new(|cs| WebRTCConnectionSetupWasm::new(cs));
    let my_storage = Box::new(DummyDS {});
    let ws = WebSocketWasm::new(url)?;
    Ok(Node::new(my_storage, "node",  Box::new(ws), rtc_spawner)?)
}

async fn list_ping(n: &mut Node) -> Result<(), NodeError> {
    n.list()?;
    n.ping().await?;
    let mut nodes: Vec<StatNode> = n.stats()?.iter().map(|(_k, v)| v.clone()).collect();
    nodes.sort_by(|a, b| b.last_contact.partial_cmp(&a.last_contact).unwrap());
    for node in nodes {
        if let Some(info) = node.node_info.as_ref() {
            if n.info()?.get_id() != info.get_id() {
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
            match node.get_messages(){
                Ok(msgs) => trace!("Got messages: {:?}", msgs),
                Err(e) => error!("While getting messages: {:?}", e),
            }
        }
        wait_ms(1000).await;
    }
}
