use thiserror::Error;

pub mod config;
pub mod network;
pub mod network_broker;
pub mod signal;
pub mod web_rtc;
pub mod websocket;

pub use flmodules::broker;
pub use flmodules::nodeids::{NodeID, NodeIDs, U256};

#[cfg(any(feature = "libc", feature = "wasm"))]
use crate::{
    config::NodeConfig,
    network_broker::{NetworkBroker, NetworkMessage},
    web_rtc::WebRTCConn,
};

#[derive(Error, Debug)]
pub enum NetworkSetupError {
    #[error(transparent)]
    Broker(#[from] flmodules::broker::BrokerError),
    #[error(transparent)]
    WebSocketClient(#[from] websocket::WSClientError),
    #[cfg(feature = "libc")]
    #[error(transparent)]
    WebSocketServer(#[from] websocket::WSSError),
    #[error(transparent)]
    Network(#[from] network_broker::NetworkError),
}

#[cfg(feature = "testing")]
pub mod testing;

#[cfg(all(feature = "libc", feature = "wasm"))]
std::compile_error!("flnet cannot have 'libc' and 'wasm' feature simultaneously");

#[cfg(feature = "libc")]
mod arch {
    use super::*;
    mod libc;
    pub use libc::*;
    use web_rtc_setup::web_rtc_spawner;
    use web_socket_client::WebSocketClient;

    pub async fn network_broker_start(
        node_config: NodeConfig,
        signal_url: &str,
    ) -> Result<broker::Broker<NetworkMessage>, NetworkSetupError> {
        let webrtc = WebRTCConn::new(web_rtc_spawner()).await?;
        let ws = WebSocketClient::connect(signal_url).await?;
        Ok(NetworkBroker::start(node_config.clone(), ws, webrtc).await?)
    }
}

#[cfg(feature = "wasm")]
mod arch {
    use super::*;
    mod wasm;
    pub use wasm::*;
    use web_rtc_setup::web_rtc_spawner;
    use web_socket_client::WebSocketClient;

    pub async fn network_broker_start(
        node_config: NodeConfig,
        signal_url: &str,
    ) -> Result<broker::Broker<NetworkMessage>, NetworkSetupError> {
        let webrtc = WebRTCConn::new(web_rtc_spawner()).await?;
        let ws = WebSocketClient::connect(signal_url).await?;
        Ok(NetworkBroker::start(node_config.clone(), ws, webrtc).await?)
    }
}

#[cfg(any(feature = "libc", feature = "wasm"))]
pub use arch::*;

#[cfg(any(feature = "libc", feature = "wasm"))]
pub async fn network_start(
    node_config: NodeConfig,
    signal_url: &str,
) -> Result<network::Network, NetworkSetupError> {
    let net_broker = network_broker_start(node_config, signal_url).await?;
    Ok(network::Network::start(net_broker).await?)
}
