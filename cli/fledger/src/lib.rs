use js_sys::Date;
use log::{error, info, trace};
use thiserror::Error;
use wasm_bindgen::{prelude::*};

use flnet::signal::websocket::WSError;
use flnet_wasm::{web_rtc_setup::WebRTCConnectionSetupWasm, web_socket::WebSocketWasm};
use flnode::node::{stats::StatNode, version::VERSION_STRING, Node, NodeError};
use flutils::{data_storage::{DataStorageBaseImpl}, time::wait_ms};

#[cfg(not(feature = "local"))]
const URL: &str = "wss://signal.fledg.re";

#[cfg(feature = "local")]
const URL: &str = "ws://localhost:8765";

#[derive(Debug, Error)]
enum StartError {
    #[error(transparent)]
    Node(#[from] NodeError),
    #[error(transparent)]
    WS(#[from] WSError),
}

async fn start(url: &str) -> Result<Node, StartError> {
    let rtc_spawner = Box::new(WebRTCConnectionSetupWasm::new_box);
    let my_storage = Box::new(DataStorageBaseImpl {});
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
            match node.get_chat_messages() {
                Ok(msgs) => trace!("Got messages: {:?}", msgs),
                Err(e) => error!("While getting messages: {:?}", e),
            }
        }
        wait_ms(1000).await;
    }
}
