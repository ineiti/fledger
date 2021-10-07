use common::node::network::node_connection::{NCError, NodeConnection};
use std::sync::{mpsc::channel, Arc, Mutex};
use wasm_bindgen_test::*;
use wasm_webrtc::helpers::wait_ms;

use common::{
    broker::{BInput, Broker, BrokerMessage, Subsystem},
    node::network::{connection_state::CSError, BrokerNetwork},
    signal::web_rtc::{WSSignalMessage, WebRTCSpawner},
    types::U256,
};
use wasm_webrtc::web_rtc_setup::WebRTCConnectionSetupWasm;

#[wasm_bindgen_test]
async fn test_node_connection() {
    console_error_panic_hook::set_once();

    wasm_logger::init(wasm_logger::Config::new(log::Level::Trace));

    log::debug!("Running test");
    if let Err(e) = test_node_connection_result().await {
        log::error!("Error during test: {:?}", e);
    }
    log::debug!("Done testing");
}

async fn test_node_connection_result() -> Result<(), NCError> {
    let id_init = U256::rnd();
    let id_follow = U256::rnd();
    let mut broker_init = Broker::new();
    let (tap_tx, tap_init) = channel::<BInput>();
    broker_init
        .add_subsystem(Subsystem::Tap(tap_tx))
        .map_err(|_| CSError::InputQueue)?;
    let web_rtc_init = Arc::new(Mutex::new(
        Box::new(|cs| WebRTCConnectionSetupWasm::new(cs)) as WebRTCSpawner,
    ));
    let mut init = NodeConnection::new(
        web_rtc_init,
        broker_init.clone(),
        id_init.clone(),
        id_follow.clone(),
    )
    .await?;

    let mut broker_follow = Broker::new();
    let (tap_tx, tap_follow) = channel::<BInput>();
    broker_follow
        .add_subsystem(Subsystem::Tap(tap_tx))
        .map_err(|_| CSError::InputQueue)?;
    let web_rtc_follow = Arc::new(Mutex::new(
        Box::new(|cs| WebRTCConnectionSetupWasm::new(cs)) as WebRTCSpawner,
    ));
    let mut follow = NodeConnection::new(
        web_rtc_follow,
        broker_follow.clone(),
        id_follow.clone(),
        id_init.clone(),
    )
    .await?;

    log::info!("Info message");
    for i in 0..5 {
        wait_ms(1000).await;
        broker_init.process().map_err(|_| CSError::InputQueue)?;
        broker_follow.process().map_err(|_| CSError::InputQueue)?;
        let msgs: Vec<BInput> = tap_init.try_iter().collect();
        log::info!("Processing messages from init");
        for msg in msgs {
            if let BInput::BM(bm) = msg {
                match bm {
                    BrokerMessage::Network(BrokerNetwork::WebSocket(
                        WSSignalMessage::PeerSetup(pi),
                    )) => {
                        log::debug!("Time {}: init sent message: {}", i, pi.message);
                        follow.process_ws(pi).await?
                    }
                    _ => {}
                }
            }
        }

        let msgs: Vec<BInput> = tap_follow.try_iter().collect();
        log::info!("Processing messages from follow");
        for msg in msgs {
            if let BInput::BM(bm) = msg {
                match bm {
                    BrokerMessage::Network(BrokerNetwork::WebSocket(
                        WSSignalMessage::PeerSetup(pi),
                    )) => {
                        log::debug!("Time {}: follow sent message: {}", i, pi.message);
                        init.process_ws(pi).await?
                    }
                    _ => {}
                }
            }
        }

        log::info!("Processing done");
    }

    init.send("Hello".into()).await?;
    wait_ms(1000).await;
    for msg in tap_follow.try_iter() {
        if let BInput::BM(BrokerMessage::NodeMessage(nm)) = msg {
            log::info!("Follow got {} / {:?}", nm.id, nm.msg);
        }
    }

    Ok(())
}

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);
