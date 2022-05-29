use flnet::{
    config::NodeConfig,
    network::{NetReply, Network, NetworkMessage, NetCall},
    signal::{server::SignalServer, websocket::WSError},
};
use flnet_libc::{
    web_rtc_setup::WebRTCConnectionSetupLibc, web_socket_client::WebSocketClient,
    web_socket_server::WebSocketServer,
};
use flmodules::{broker::Broker, nodeids::U256};
use flarch::{start_logging_filter};
use thiserror::Error;

const URL: &str = "ws://localhost:8765";

#[derive(Debug, Error)]
enum MainError {
    #[error(transparent)]
    WS(#[from] WSError),
    #[error(transparent)]
    Broker(#[from] flmodules::broker::BrokerError),
}

#[tokio::main]
async fn main() -> Result<(), MainError> {
    start_logging_filter(vec!["libc", "broker", "flnet_libc", "flnet"]);

    log::info!("Starting signalling server");
    start_signal_server().await;

    log::debug!("Starting node 1");
    let (_nc1, mut broker1) = spawn_node().await?;

    let (tap1, _) = broker1.get_tap().await.expect("Failed to get tap");

    for msg in tap1 {
        log::debug!("Node 1: {msg:?}");
        if let NetworkMessage::Reply(NetReply::RcvNodeMessage(nm)) = msg {
            log::info!("Got message from other node: {}", nm.1);
            send(&mut broker1, nm.0, "Reply from libc").await;
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
        nc.our_node.get_id(),
        nc.our_node.info
    );
    log::debug!("Connecting to websocket at {URL}");
    let ws = WebSocketClient::connect(URL)
        .await
        .expect("Failed to connect to signalling server");

    log::debug!("Starting network");
    let net = Network::start(
        nc.clone(),
        ws,
        Box::new(|| Box::new(Box::pin(WebRTCConnectionSetupLibc::new_box()))),
    )
    .await
    .expect("Starting node failed");

    Ok((nc, net))
}

async fn send(src: &mut Broker<NetworkMessage>, id: U256, msg: &str) {
    src.emit_msg(
        NetworkMessage::Call(NetCall::SendNodeMessage((id, msg.into())))
    )
    .await
    .expect("Sending to node");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::Receiver;

    use flarch::arch::wait_ms;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_two_nodes() -> Result<(), MainError> {
        start_logging_filter(vec!["libc", "broker", "flnet_libc", "flnet"]);

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
            send(&mut broker1, nc2.our_node.get_id(), "Message 1").await;
            wait_msg(&tap2, "Message 1").await;
            log::debug!("Sending second message");
            send(&mut broker2, nc1.our_node.get_id(), "Message 2").await;
            wait_msg(&tap1, "Message 2").await;
        }
        Ok(())
    }

    async fn wait_msg(tap: &Receiver<NetworkMessage>, msg: &str) {
        for msg_net in tap {
            if let NetworkMessage::Reply(NetReply::RcvNodeMessage(nm)) = &msg_net {
                if nm.msg == msg {
                    break;
                }
            }
        }
    }
}
