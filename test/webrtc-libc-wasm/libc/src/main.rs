use thiserror::Error;

use flnet::{
    network_start,
    NetworkSetupError,
    config::NodeConfig,
    network::{NetReply, NetworkMessage, NetCall},
    signal::SignalServer,
    web_socket_server::WebSocketServer,
};
use flmodules::{broker::Broker, nodeids::U256};
use flarch::{start_logging_filter};

const URL: &str = "ws://localhost:8765";

#[derive(Debug, Error)]
enum MainError {
    #[error(transparent)]
    NetworkSetup(#[from] NetworkSetupError),
    #[error(transparent)]
    Broker(#[from] flmodules::broker::BrokerError),
}

#[tokio::main]
async fn main() -> Result<(), MainError> {
    start_logging_filter(vec!["libc", "broker", "flnet"]);

    log::info!("Starting signalling server");
    start_signal_server().await;

    log::debug!("Starting node 1");
    let (_nc1, mut broker1) = spawn_node().await?;

    let (tap1, _) = broker1.get_tap().await.expect("Failed to get tap");

    for msg in tap1 {
        log::debug!("Node 1: {msg:?}");
        if let NetworkMessage::Reply(NetReply::RcvNodeMessage(id, msg_net)) = msg {
            log::info!("Got message from other node: {}", msg_net);
            send(&mut broker1, id, "Reply from libc").await;
        }
    }

    Ok(())
}

async fn start_signal_server() {
    let wss = WebSocketServer::new(8765)
        .await
        .expect("Failed to start signalling server");
    log::debug!("Starting signalling server");
    SignalServer::new(wss, 0)
        .await
        .expect("Failed to start signalling server");
}

async fn spawn_node() -> Result<(NodeConfig, Broker<NetworkMessage>), MainError> {
    let nc = NodeConfig::new();

    log::info!(
        "Starting node {}: {}",
        nc.info.get_id(),
        nc.info.name
    );
    log::debug!("Connecting to websocket at {URL}");
    let net = network_start(nc.clone(), URL)
    .await
    .expect("Starting node failed");

    Ok((nc, net))
}

async fn send(src: &mut Broker<NetworkMessage>, id: U256, msg: &str) {
    src.emit_msg(
        NetworkMessage::Call(NetCall::SendNodeMessage(id, msg.into()))
    )
    .expect("Sending to node");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::Receiver;

    use flarch::wait_ms;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_two_nodes() -> Result<(), MainError> {
        start_logging_filter(vec!["libc", "broker", "flnet"]);

        log::info!("Starting signalling server");
        start_signal_server().await;
        log::debug!("Starting node 1");
        let (nc1, mut broker1) = spawn_node().await?;
        log::debug!("Starting node 2");
        let (nc2, mut broker2) = spawn_node().await?;
        wait_ms(1000).await;
        let (tap2, _) = broker2.get_tap().await.expect("Failed to get tap");
        let (tap1, _) = broker1.get_tap().await.expect("Failed to get tap");
        for _ in 1..=2 {
            log::debug!("Sending first message");
            send(&mut broker1, nc2.info.get_id(), "Message 1").await;
            wait_msg(&tap2, "Message 1").await;
            log::debug!("Sending second message");
            send(&mut broker2, nc1.info.get_id(), "Message 2").await;
            wait_msg(&tap1, "Message 2").await;
        }
        Ok(())
    }

    async fn wait_msg(tap: &Receiver<NetworkMessage>, msg: &str) {
        for msg_net in tap {
            if let NetworkMessage::Reply(NetReply::RcvNodeMessage(_, nm)) = &msg_net {
                if nm == msg {
                    break;
                }
            }
        }
    }
}
