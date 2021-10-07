use std::sync::{mpsc::channel, Arc, Mutex};
use wasm_bindgen_test::*;
use wasm_webrtc::helpers::wait_ms;

use common::{
    broker::{BInput, Broker, BrokerMessage, Subsystem},
    node::network::{
        connection_state::{CSError, ConnectionState},
        BrokerNetwork,
    },
    signal::web_rtc::{WSSignalMessage, WebRTCSpawner},
    types::U256,
};
use wasm_webrtc::web_rtc_setup::WebRTCConnectionSetupWasm;

#[wasm_bindgen_test]
async fn test_connection_state() {
    console_error_panic_hook::set_once();
    wasm_logger::init(wasm_logger::Config::new(log::Level::Trace));

    log::debug!("Running test");
    if let Err(e) = test_connection_state_result().await {
        log::error!("Error during test: {:?}", e);
    }
}

async fn test_connection_state_result() -> Result<(), CSError> {
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
    let mut init = ConnectionState::new(
        false,
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
    let mut follow = ConnectionState::new(
        true,
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
        for msg in tap_init.try_iter() {
            if let BInput::BM(bm) = msg {
                match bm {
                    BrokerMessage::Network(BrokerNetwork::WebSocket(
                        WSSignalMessage::PeerSetup(pi),
                    )) => {
                        log::debug!("Time {}: init sent message: {}", i, pi.message);
                        follow.process_peer_message(pi.message).await?
                    }
                    _ => {}
                }
            }
        }
        for msg in tap_follow.try_iter() {
            if let BInput::BM(bm) = msg {
                match bm {
                    BrokerMessage::Network(BrokerNetwork::WebSocket(
                        WSSignalMessage::PeerSetup(pi),
                    )) => {
                        log::debug!("Time {}: follow sent message: {}", i, pi.message);
                        init.process_peer_message(pi.message).await?
                    }
                    _ => {}
                }
            }
        }
    }

    init.send("Hello".into()).await?;
    wait_ms(1000).await;
    for msg in tap_follow.try_iter() {
        if let BInput::BM(BrokerMessage::NodeMessage(m)) = msg {
            log::info!("Follow got {} / {:?}", m.id, m.msg);
        }
    }

    Ok(())
}
