use flmodules::broker::Broker;
use flnet::{config::NodeConfig, network::{NetworkMessage, Network}, web_rtc::WebRTCConn};
use thiserror::Error;
use web_rtc::web_rtc_spawner;
use web_socket_client::WebSocketClient;

pub mod web_rtc;
pub mod web_rtc_setup;
pub mod web_socket_client;


#[derive(Error, Debug)]
pub enum NetworkSetupError {
    #[error(transparent)]
    Broker(#[from] flmodules::broker::BrokerError),
    #[error(transparent)]
    WebSocket(#[from] web_socket_client::WSClientError),
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
