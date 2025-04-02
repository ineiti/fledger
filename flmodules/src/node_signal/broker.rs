use flarch::web_rtc::{connection::ConnectionConfig, web_rtc_setup::web_rtc_spawner, WebRTCConn};
use tokio::sync::watch;

use crate::{
    network::broker::{BrokerNetwork, Network},
    nodeconfig::NodeConfig,
    router::broker::BrokerRouter,
};
use flarch::{broker::Broker, nodeids::NodeID};

use super::messages::{InternIn, InternOut, Messages, NodeSignalStats};

pub(super) const MODULE_NAME: &str = "NodeSignal";

/// This links the NodeSignal module with other modules, so that
/// all messages are correctly translated from one to the other.
/// For this example, it uses the RandomConnections module to communicate
/// with other nodes.
///
/// The [NodeSignal] holds the [Translate] and offers convenience methods
/// to interact with [Translate] and [NodeSignalMessage].
#[derive(Clone)]
pub struct NodeSignal {
    /// Represents the underlying broker.
    pub broker: BrokerRouter,
    _our_id: NodeID,
    pub storage: watch::Receiver<NodeSignalStats>,
}

impl NodeSignal {
    pub async fn start(
        connection: ConnectionConfig,
        node_config: NodeConfig,
        net_signal: BrokerNetwork,
    ) -> anyhow::Result<Self> {
        let (messages, storage) = Messages::new(node_config.info.get_id());
        let mut intern = Broker::new();
        intern.add_handler(Box::new(messages)).await?;

        let web_rtc = WebRTCConn::new(web_rtc_spawner(connection)).await?;
        let web_socket = Broker::new();
        let net_node = Network::start(node_config.clone(), web_socket.clone(), web_rtc).await?;

        intern
            .add_translator_link(
                web_socket,
                Box::new(|msg| match msg {
                    InternOut::WebSocket(wsclient_in) => Some(wsclient_in),
                    _ => None,
                }),
                Box::new(|msg| Some(InternIn::WebSocket(msg))),
            )
            .await?;

        intern
            .add_translator_link(
                net_node.broker.clone(),
                Box::new(|msg| match msg {
                    InternOut::NetworkNode(network_in) => Some(network_in),
                    _ => None,
                }),
                Box::new(|msg| Some(InternIn::NetworkNode(msg))),
            )
            .await?;

        intern
            .add_translator_link(
                net_signal.clone(),
                Box::new(|msg| match msg {
                    InternOut::NetworkSignal(network_in) => Some(network_in),
                    _ => None,
                }),
                Box::new(|msg| Some(InternIn::NetworkSignal(msg))),
            )
            .await?;

        let broker = Broker::new();
        intern
            .add_translator_direct(
                broker.clone(),
                Box::new(|msg| match msg {
                    InternOut::Router(router_out) => Some(router_out),
                    _ => None,
                }),
                Box::new(|msg| Some(InternIn::Router(msg))),
            )
            .await?;

        Ok(NodeSignal {
            broker,
            _our_id: node_config.info.get_id(),
            storage,
        })
    }
}

#[cfg(test)]
mod tests {}
