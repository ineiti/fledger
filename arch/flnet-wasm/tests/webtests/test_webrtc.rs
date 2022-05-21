use std::sync::mpsc::Receiver;

use flnet::signal::web_rtc::{PeerMessage, SetupError, WebRTCInput, WebRTCMessage, WebRTCOutput};
use flnet_wasm::{web_rtc::WebRTCConnection};
use flarch::{
    broker::{Broker, Destination, BrokerError},
    arch::wait_ms,
};
use thiserror::Error;
use wasm_bindgen_test::wasm_bindgen_test;

#[derive(Error, Debug)]
enum TestError {
    #[error(transparent)]
    WebSocket(#[from] flnet_wasm::web_socket_client::WSClientError),
    #[error(transparent)]
    Network(#[from] flnet::network::NetworkError),
    #[error(transparent)]
    Broker(#[from] flmodules::broker::BrokerError),
    #[error(transparent)]
    Logger(#[from] log::SetLoggerError),
    #[error(transparent)]
    Setup(#[from] flnet::signal::web_rtc::SetupError),
}

#[wasm_bindgen_test]
async fn test_webrtc() {
    femme::with_level(femme::LevelFilter::Trace);
    
    test_webrtc_error().await.err().map(|e| {
        log::error!("{:?}", e);
        assert!(false);
    });
}

async fn test_webrtc_error() -> Result<(), TestError> {
    // femme::with_level(femme::LevelFilter::Trace);

    let mut init = WebRTCConnection::new_box().await?;
    let mut follow = WebRTCConnection::new_box().await?;
    log::debug!("Started init and follow node");

    let (init_tap, _) = init.get_tap().await?;
    let (follow_tap, _) = follow.get_tap().await?;

    init.link_bi(follow.clone(), Box::new(link_webrtc), Box::new(link_webrtc))
        .await;
    log::debug!("Sending init message");
    init.emit_msg(WebRTCMessage::Input(WebRTCInput::Setup(PeerMessage::Init)))
        .await?;

    let mut connections = 0;
    while connections < 2 {
        for msg in init_tap.try_iter() {
            log::info!("init:  {:?}", msg);
            if matches!(msg, WebRTCMessage::Output(WebRTCOutput::Connected)) {
                connections += 1;
            }
        }
        for msg in follow_tap.try_iter() {
            log::info!("follow: {:?}", msg);
            if matches!(msg, WebRTCMessage::Output(WebRTCOutput::Connected)) {
                connections += 1;
            }
        }
        wait_ms(100).await;
    }

    check_connected(init, follow).await?;

    Ok(())
}

fn link_webrtc(msg: WebRTCMessage) -> Option<WebRTCMessage> {
    if let WebRTCMessage::Output(WebRTCOutput::Setup(m)) = msg {
        log::debug!("Link setup message: {:?}", m);
        Some(WebRTCMessage::Input(WebRTCInput::Setup(m)))
    } else {
        None
    }
}

async fn check_connected(
    mut init: Broker<WebRTCMessage>,
    mut follow: Broker<WebRTCMessage>,
) -> Result<(), SetupError> {
    let (init_tap, _) = init.get_tap().await?;
    let (follow_tap, _) = follow.get_tap().await?;

    wait_msg(&mut init, follow_tap, true).await?;

    wait_msg(&mut follow, init_tap, false).await?;

    log::debug!("Got message from follow");
    Ok(())
}

async fn wait_msg(src_broker: &mut Broker<WebRTCMessage>, tap: Receiver<WebRTCMessage>, init: bool) -> Result<(), BrokerError> {    
    let (src, dst) = if init {
        ("init", "follow")
    } else {
        ("follow", "init")
    };
    let msg_send = format!("{src}->{dst}");

    log::info!("Sending {msg_send}");
    src_broker.emit_msg_dest(
        Destination::NoTap,
        WebRTCMessage::Input(WebRTCInput::Text(msg_send.clone())),
    )
    .await?;

    for i in 0..=10 {
        if let Some(msg) = tap.try_iter().next() {
            log::info!("{dst}: {msg:?}");
            if let WebRTCMessage::Output(WebRTCOutput::Text(m)) = msg {
                assert_eq!(m, msg_send);
                break;
            }
        }
        if i == 10 {
            panic!("Didn't receive message");
        }
        wait_ms(100).await;
    }
    log::debug!("Got message from {src}");

    Ok(())
}
