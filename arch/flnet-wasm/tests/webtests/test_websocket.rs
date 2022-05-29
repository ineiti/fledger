use flnet::{
    config::NodeConfig,
    network::{NetworkError, NetworkMessage},
    websocket::{WSClientMessage, WSClientOutput},
};
use flnet_wasm::{network_start, NetworkSetupError};
use flarch::{tasks::wait_ms};
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
    Setup(#[from] NetworkSetupError)
}

#[wasm_bindgen_test]
async fn test_websocket() {
    test_websocket_error().await.err().map(|e| {
        log::error!("{:?}", e);
        assert!(false);
    });
}

async fn test_websocket_error() -> Result<(), TestError> {
    // wasm_logger::init(wasm_logger::Config::default());
    // femme::with_level(femme::LevelFilter::Trace);

    let nc = NodeConfig::new();

    let mut net = network_start(nc, "ws://localhost:8765").await?;

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
