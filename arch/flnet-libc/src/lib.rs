use flmodules::broker::Broker;
use flnet::{
    config::NodeConfig,
    network::{Network, NetworkMessage},
    web_rtc::WebRTCConn,
};
use thiserror::Error;
use web_rtc_setup::web_rtc_spawner;
use web_socket_client::WebSocketClient;

pub mod data_storage;
pub mod web_rtc_setup;
pub mod web_socket_client;
pub mod web_socket_server;

#[derive(Error, Debug)]
pub enum NetworkSetupError {
    #[error(transparent)]
    Broker(#[from] flmodules::broker::BrokerError),
    #[error(transparent)]
    WebSocket(#[from] web_socket_server::WSSError),
    #[error(transparent)]
    Network(#[from] flnet::network::NetworkError)
}

pub async fn network_start(
    node_config: NodeConfig,
    signal_url: &str,
) -> Result<Broker<NetworkMessage>, NetworkSetupError> {
    let webrtc = WebRTCConn::new(web_rtc_spawner()).await?;
    let ws = WebSocketClient::connect(signal_url).await?;
    Ok(Network::start(node_config.clone(), ws, webrtc).await?)
}
