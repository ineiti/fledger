use flnet_wasm::{network_start};
use flnode::{node::Node};
use flarch::{data_storage::{DataStorage, DataStorageNode}, wait_ms};
use wasm_bindgen::{prelude::*};

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
    let storage = DataStorageNode::new("fledger".into());
    let node_config = Node::get_config(storage.clone())?;

    log::info!("Starting app with version {}", VERSION_STRING);

    log::debug!("Connecting to websocket at {URL}");
    let network = network_start(node_config.clone(), URL).await?;

    let mut node = Node::start(Box::new(storage), node_config, network).await?;
    let nc = &node.node_config.info;
    log::info!("Starting node {}: {:?}", nc.get_id(), nc.name);

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
            let ping = node.ping.storage.clone();
            log::info!("Nodes countdowns are: {:?}", ping.stats);
            log::info!("Chat messages are: {:?}", node.gossip.chat_events());
        }
        wait_ms(1000).await;
    }
}
