use broker::{BrokerRouter, RouterNetwork};
use flarch::web_rtc::{
    connection::ConnectionConfig, web_rtc_setup::web_rtc_spawner,
    web_socket_client::WebSocketClient, WebRTCConn,
};

use crate::nodeconfig::NodeConfig;

pub mod broker;
pub mod messages;

/// Starts a new [`broker::Broker<NetworkMessage>`] with a given `node`- and `connection`-configuration.
/// This returns a raw broker which is mostly suited to connect to other brokers.
/// If you need an easier access to the WebRTC network, use [`network_start`], which returns
/// a structure with a more user-friendly API.
///
/// # Example
///
/// ```bash
/// async fn start_network() -> anyhow::Result<()>{
///   let net = network_broker_start();
/// }
/// ```
pub async fn router_broker_start(
    node: NodeConfig,
    connection: ConnectionConfig,
) -> anyhow::Result<BrokerRouter> {
    use crate::network::broker::Network;

    let ws = WebSocketClient::connect(&connection.signal()).await?;
    let webrtc = WebRTCConn::new(web_rtc_spawner(connection)).await?;
    let net = Network::start(node.clone(), ws, webrtc).await?;
    Ok(RouterNetwork::start(net.broker).await?)
}
