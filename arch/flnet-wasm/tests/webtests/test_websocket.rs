use flnet::{
    config::NodeConfig,
    network::{Network, NetworkError, NetworkMessage},
    signal::{web_rtc::{SetupError, WebRTCMessage}, websocket::{WSClientMessage, WSClientOutput}},
};
use flnet_wasm::web_socket_client::WebSocketClient;
use flarch::{broker::Broker, arch::wait_ms};
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
}

#[wasm_bindgen_test]
async fn test_websocket() {
    test_websocket_error().await.err().map(|e| {
        log::error!("{:?}", e);
        assert!(false);
    });
}

pub async fn dummy_webrtc() -> Result<Broker<WebRTCMessage>, SetupError> {
    Ok(Broker::new())
}

async fn test_websocket_error() -> Result<(), TestError> {
    // wasm_logger::init(wasm_logger::Config::default());
    // femme::with_level(femme::LevelFilter::Trace);

    let ws = WebSocketClient::connect("ws://localhost:8765").await?;
    let nc = NodeConfig::new();

    let mut net = Network::start(nc, ws, Box::new(|| Box::new(Box::pin(dummy_webrtc())))).await?;

    let (net_tap, _) = net.get_tap().await?;
    for i in 0..10 {
        log::debug!("Looping {i}");
        for msg in net_tap.try_iter() {
            log::info!("{:?}", msg);
            if let NetworkMessage::WebSocket(WSClientMessage::Output(WSClientOutput::Message(txt))) = msg {
                if txt.contains("ListIDsReply"){
                    return Ok(());
                }
            }
        }
        wait_ms(100).await;
    }
    Err(NetworkError::ConnectionMissing.into())
}
