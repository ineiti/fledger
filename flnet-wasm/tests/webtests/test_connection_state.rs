use std::sync::{mpsc::channel, Arc, Mutex};
use wasm_bindgen_test::*;

use flnet::{
    network::{
        connection_state::{CSError, ConnectionState},
        BrokerNetwork, BrokerNetworkCall, BrokerNetworkReply,
    },
    signal::web_rtc::WebRTCSpawner,
};
use flnet_wasm::web_rtc_setup::WebRTCConnectionSetupWasm;
use flutils::{
    broker::{Broker, Subsystem},
    nodeids::U256,
    time::wait_ms,
};

use super::TestError;

#[wasm_bindgen_test]
async fn test_connection_state() {
    console_error_panic_hook::set_once();
    wasm_logger::init(wasm_logger::Config::new(log::Level::Trace));

    log::debug!("Running test");
    if let Err(e) = test_connection_state_result().await {
        log::error!("Error during test: {:?}", e);
    }
}

async fn test_connection_state_result() -> Result<(), TestError> {
    let id_init = U256::rnd();
    let id_follow = U256::rnd();
    let mut broker_init = Broker::new();
    let (tap_tx, tap_init) = channel::<BrokerNetwork>();
    broker_init.add_subsystem(Subsystem::Tap(tap_tx))?;
    let web_rtc_init = Arc::new(Mutex::new(
        Box::new(WebRTCConnectionSetupWasm::new_box) as WebRTCSpawner
    ));
    let mut init =
        ConnectionState::new(false, web_rtc_init, broker_init.clone(), id_init, id_follow).await?;

    let mut broker_follow = Broker::new();
    let (tap_tx, tap_follow) = channel::<BrokerNetwork>();
    broker_follow.add_subsystem(Subsystem::Tap(tap_tx))?;
    let web_rtc_follow = Arc::new(Mutex::new(
        Box::new(WebRTCConnectionSetupWasm::new_box) as WebRTCSpawner
    ));
    let mut follow = ConnectionState::new(
        true,
        web_rtc_follow,
        broker_follow.clone(),
        id_follow,
        id_init,
    )
    .await?;

    log::info!("Info message");
    for i in 0..5 {
        wait_ms(1000).await;
        broker_init.process().map_err(|_| CSError::InputQueue)?;
        broker_follow.process().map_err(|_| CSError::InputQueue)?;
        // TODO: rewrite tests
        for msg in tap_init.try_iter() {
            if let BrokerNetwork::Call(BrokerNetworkCall::SendWSPeer(pi)) = msg {
                log::debug!("Time {}: init sent message: {}", i, pi.message);
                follow.process_peer_message(pi.message).await?
            }
        }
        for msg in tap_follow.try_iter() {
            if let BrokerNetwork::Call(BrokerNetworkCall::SendWSPeer(pi)) = msg {
                log::debug!("Time {}: follow sent message: {}", i, pi.message);
                init.process_peer_message(pi.message).await?
            }
        }
    }

    init.send("Hello".into()).await?;
    wait_ms(1000).await;
    // TODO: rewrite tests
    for msg in tap_follow.try_iter() {
        if let BrokerNetwork::Reply(BrokerNetworkReply::RcvNodeMessage(m)) = msg {
            log::info!("Follow got {} / {:?}", m.id, m.msg);
        }
    }

    Ok(())
}
