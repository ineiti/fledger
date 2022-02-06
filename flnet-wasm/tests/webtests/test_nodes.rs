use anyhow::{anyhow, Result};
use flnode::node::{node_data::TempDS, Node};
use wasm_bindgen_test::*;
use wasm_webrtc::{
    helpers::wait_ms, web_rtc_setup::WebRTCConnectionSetupWasm, web_socket::WebSocketWasm,
};

#[wasm_bindgen_test]
async fn test_nodes_2() {
    log::debug!("Testing nodes - 2");
}

#[wasm_bindgen_test]
async fn test_nodes() {
    console_error_panic_hook::set_once();

    wasm_logger::init(wasm_logger::Config::new(log::Level::Trace));

    log::debug!("Running test");
    if let Err(e) = test_nodes_result().await {
        log::error!("Error during test: {:?}", e);
    }
    log::debug!("Done testing");
}

async fn test_nodes_result() -> Result<()> {
    log::info!("This test supposes that a signal server is running on the localhost!");

    let storage1 = TempDS::new();
    let web_rtc = Box::new(|cs| WebRTCConnectionSetupWasm::new(cs));
    let ws = WebSocketWasm::new("ws://localhost:8765")
        .map_err(|e| anyhow!("couldn't create websocket: {:?}", e))?;
    let node1 = Node::new(storage1, "test", Box::new(ws), web_rtc)
        .map_err(|e| anyhow!("couldn't create node: {:?}", e))?;

    let storage2 = TempDS::new();
    let web_rtc = Box::new(|cs| WebRTCConnectionSetupWasm::new(cs));
    let ws = WebSocketWasm::new("ws://localhost:8765")
        .map_err(|e| anyhow!("couldn't create websocket: {:?}", e))?;
    let _ = Node::new(storage2, "test", Box::new(ws), web_rtc)
        .map_err(|e| anyhow!("couldn't create node: {:?}", e))?;

    for i in 0..5 {
        wait_ms(1000).await;
        log::info!("Stats at time {}", i);
        log::info!("node1: {:?}", node1.stats()?);
    }

    Ok(())
}

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);
