use thiserror::Error;

use flarch::{
    broker::{Broker, BrokerError},
    nodeids::U256,
    start_logging_filter,
    web_rtc::{connection::ConnectionConfig, web_socket_server::WebSocketServer},
};
use flmodules::nodeconfig::NodeConfig;
use flnet::{
    network::{NetCall, NetReply, NetworkMessage},
    network_broker_start,
    signal::SignalServer,
    NetworkSetupError,
};

const URL: &str = "ws://127.0.0.1:8765";

#[derive(Debug, Error)]
enum MainError {
    #[error(transparent)]
    NetworkSetup(#[from] NetworkSetupError),
    #[error(transparent)]
    Broker(#[from] BrokerError),
}

#[tokio::main]
async fn main() -> Result<(), MainError> {
    start_logging_filter(vec!["fl"]);

    log::info!("Starting signalling server");
    start_signal_server().await;

    let (nc, mut broker) = spawn_node().await?;
    log::info!("Starting node: {} / {}", nc.info.name, nc.info.get_id());

    let (mut tap, _) = broker.get_tap().await.expect("Failed to get tap");
    let mut msgs_rcv = 0;

    loop {
        let msg = tap.recv().await.expect("expected message");
        if let NetworkMessage::Reply(NetReply::RcvNodeMessage(id, msg_net)) = msg {
            log::info!("Got message from other node: {}", msg_net);
            if msgs_rcv == 0 {
                msgs_rcv += 1;
                send(&mut broker, id, "Reply from libc").await;
            } else {
                return Ok(());
            }
        }
    }
}

async fn start_signal_server() {
    let wss = WebSocketServer::new(8765)
        .await
        .expect("Failed to start signalling server");
    log::debug!("Starting signalling server");
    SignalServer::new(wss, 1)
        .await
        .expect("Failed to start signalling server");
}

async fn spawn_node() -> Result<(NodeConfig, Broker<NetworkMessage>), MainError> {
    let nc = NodeConfig::new();

    log::info!("Starting node {}: {}", nc.info.get_id(), nc.info.name);
    log::debug!("Connecting to websocket at {URL}");
    let net = network_broker_start(nc.clone(), ConnectionConfig::from_signal(URL))
        .await
        .expect("Starting node failed");

    Ok((nc, net))
}

async fn send(src: &mut Broker<NetworkMessage>, id: U256, msg: &str) {
    src.emit_msg(NetworkMessage::Call(NetCall::SendNodeMessage(
        id,
        msg.into(),
    )))
    .expect("Sending to node");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::Receiver;

    use flarch::tasks::wait_ms;

    // #[tokio::test]
    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn test_two_nodes() -> Result<(), MainError> {
        start_logging_filter(vec!["fl"]);

        log::info!("Starting signalling server");
        start_signal_server().await;
        log::debug!("Starting node 1");
        let (nc1, mut broker1) = spawn_node().await?;
        log::debug!("Starting node 2");
        let (nc2, mut broker2) = spawn_node().await?;
        let (tap2, _) = broker2.get_tap_sync().await.expect("Failed to get tap");
        let (tap1, _) = broker1.get_tap_sync().await.expect("Failed to get tap");
        wait_ms(1000).await;
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
