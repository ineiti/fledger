use flnet_wasm::{web_socket_client::WebSocketClient, web_rtc::web_rtc_spawner};
use flnode::{node_data::NodeData, node::Node};
use flarch::{data_storage::{DataStorageBase}, wait_ms};
use wasm_bindgen::{prelude::*};

mod data_storage;
use data_storage::*;

#[cfg(not(feature = "local"))]
const URL: &str = "wss://signal.fledg.re";

#[cfg(feature = "local")]
const URL: &str = "ws://localhost:8765";

const VERSION_STRING: &str = "123";

#[wasm_bindgen(start)]
pub async fn main() {
    console_error_panic_hook::set_once();

    femme::with_level(femme::LevelFilter::Debug);

    log::info!("Starting app with version {}", VERSION_STRING);

    runit().await.err().map(|e| log::error!("While running main: {e:?}"));
}

async fn runit()  -> Result<(), Box<dyn std::error::Error>> {
    let storage = DummyDSB{};
    let node_config = NodeData::get_config(storage.clone())?;

    log::info!("Starting app with version {}", VERSION_STRING);

    log::debug!("Connecting to websocket at {URL}");
    let ws = WebSocketClient::connect(URL)
        .await
        .expect("Failed to connect to signalling server");

    let mut node = Node::new(Box::new(storage), node_config, "cli", ws, web_rtc_spawner()).await?;
    let nc = node.info();
    log::info!("Starting node {}: {}", nc.get_id(), nc.info);

    log::info!("Started successfully");
    let mut i: i32 = 0;
    loop {
        i += 1;
        node.process()
            .await
            .err()
            .map(|e| log::warn!("Couldn't process node: {e:?}"));

        if i % 3 == 2 {
            log::info!("Nodes are: {:?}", node.nodes_online()?);
            let ping = node.nodes_ping();
            log::info!("Nodes countdowns are: {:?}", ping.stats);
            log::info!("Chat messages are: {:?}", node.get_chat_messages());
        }
        wait_ms(1000).await;
    }
}
