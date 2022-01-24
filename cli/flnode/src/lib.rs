use common::{node::NodeError, signal::websocket::WSError};
use js_sys::Date;
use log::{error, info, trace};
use thiserror::Error;
use types::data_storage::DataStorageBase;
use types::data_storage::{DataStorage, StorageError};
use wasm_bindgen::{prelude::*, JsValue};
use wasm_webrtc::{
    helpers::wait_ms, web_rtc_setup::WebRTCConnectionSetupWasm, web_socket::WebSocketWasm,
};

use common::node::{stats::StatNode, version::VERSION_STRING, Node};

#[cfg(not(feature = "local"))]
const URL: &str = "wss://signal.fledg.re";

#[cfg(feature = "local")]
const URL: &str = "ws://localhost:8765";

#[wasm_bindgen(
    inline_js = "module.exports.fswrite = function(name, str) { fs.writeFileSync(name, str); }
    module.exports.fsread = function(name) { return fs.readFileSync(name, {encoding: 'utf-8'}); }
    module.exports.fsexists = function(name) { return fs.existsSync(name); }"
)]
extern "C" {
    pub fn fswrite(name: &str, str: &str);
    #[wasm_bindgen(catch)]
    pub fn fsread(name: &str) -> Result<String, JsValue>;
    pub fn fsexists(name: &str) -> bool;
}

struct DummyDSB {}

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

struct DummyDS {
    base: String,
}

impl DummyDS {
    fn name(&self, key: &str) -> String {
        format!("{}_{}.toml", self.base, key)
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
}

#[derive(Debug, Error)]
enum StartError {
    #[error(transparent)]
    Node(#[from] NodeError),
    #[error(transparent)]
    WS(#[from] WSError),
}

async fn start(url: &str) -> Result<Node, StartError> {
    let rtc_spawner = Box::new(WebRTCConnectionSetupWasm::new_box);
    let my_storage = Box::new(DummyDSB {});
    let ws = WebSocketWasm::new(url)?;
    Ok(Node::new(my_storage, "node", Box::new(ws), rtc_spawner)?)
}

async fn list_node(n: &mut Node) -> Result<(), NodeError> {
    n.list()?;
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
        i += 1;

        if i % 3 == 2 {
            info!("Waiting");
            if let Err(e) = list_node(&mut node).await {
                error!("Couldn't list or ping nodes: {}", e);
            }
            match node.get_messages() {
                Ok(msgs) => trace!("Got messages: {:?}", msgs),
                Err(e) => error!("While getting messages: {:?}", e),
            }
        }
        wait_ms(1000).await;
    }
}
