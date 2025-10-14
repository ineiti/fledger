use thiserror::Error;

use flarch::{
    broker::BrokerError,
    nodeids::U256,
    start_logging_filter,
    web_rtc::{connection::ConnectionConfig, web_socket_server::WebSocketServer},
};
use flmodules::{
    network::{
        broker::{BrokerNetwork, NetworkIn, NetworkOut},
        network_start,
        signal::{SignalConfig, SignalServer},
        NetworkSetupError,
    },
    timer::Timer,
};
use flmodules::{nodeconfig::NodeConfig, router::messages::NetworkWrapper};

const URL: &str = "ws://127.0.0.1:8765";
const MODULE_NAME: &str = "LIBC_WASM";

#[derive(Debug, Error)]
enum MainError {
    #[error(transparent)]
    NetworkSetup(#[from] NetworkSetupError),
    #[error(transparent)]
    Broker(#[from] BrokerError),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    start_logging_filter(vec!["fl"]);

    log::info!("Starting signalling server");
    start_signal_server().await;

    let (nc, mut broker) = spawn_node().await?;
    log::info!("Starting node: {} / {}", nc.info.name, nc.info.get_id());

    let (mut tap, _) = broker.get_tap_out().await.expect("Failed to get tap");
    let mut msgs_rcv = 0;

    loop {
        let msg = tap.recv().await.expect("expected message");
        if let NetworkOut::MessageFromNode(id, msg_net) = msg {
            log::info!("Got message from other node: {:?}", msg_net);
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
    SignalServer::start(
        wss,
        SignalConfig {
            ttl_minutes: 1,
            system_realm: None,
            max_list_len: None,
        },
    )
    .await
    .expect("Failed to start signalling server");
}

async fn spawn_node() -> anyhow::Result<(NodeConfig, BrokerNetwork)> {
    let nc = NodeConfig::new();

    log::info!("Starting node {}: {}", nc.info.get_id(), nc.info.name);
    log::debug!("Connecting to websocket at {URL}");
    let net = network_start(
        nc.clone(),
        ConnectionConfig::from_signal(URL),
        &mut Timer::start().await?,
    )
    .await
    .expect("Starting node failed");

    Ok((nc, net.broker))
}

async fn send(src: &mut BrokerNetwork, id: U256, msg: &str) {
    src.emit_msg_in(NetworkIn::MessageToNode(
        id,
        NetworkWrapper::wrap_yaml(MODULE_NAME, &msg.to_string()).expect("Creating NetworkWrapper"),
    ))
    .expect("Sending to node");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::Receiver;

    use flarch::{start_logging_filter_level, tasks::wait_ms};

    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn test_two_nodes() -> anyhow::Result<()> {
        start_logging_filter_level(vec!["fl", "webrtc_test_libc"], log::LevelFilter::Info);

        log::info!("Starting signalling server");
        start_signal_server().await;
        log::info!("Starting node 1");
        let (nc1, mut broker1) = spawn_node().await?;
        log::info!("Starting node 2");
        let (nc2, mut broker2) = spawn_node().await?;
        let (tap2, _) = broker2.get_tap_out_sync().await.expect("Failed to get tap");
        let (tap1, _) = broker1.get_tap_out_sync().await.expect("Failed to get tap");
        wait_ms(2000).await;
        for _ in 1..=2 {
            log::info!("Sending first message");
            let msg1 = "Message1".to_string();
            send(&mut broker1, nc2.info.get_id(), &msg1).await;
            wait_msg(&tap2, &msg1).await;
            log::info!("Sending second message");
            let msg2 = "Message2".to_string();
            send(&mut broker2, nc1.info.get_id(), &msg2).await;
            wait_msg(&tap1, &msg2).await;
        }
        Ok(())
    }

    async fn wait_msg(tap: &Receiver<NetworkOut>, msg: &String) {
        for msg_net in tap {
            if let NetworkOut::MessageFromNode(_, nm) = &msg_net {
                if let Some(msg_mod) = nm.unwrap_yaml::<String>(MODULE_NAME) {
                    if &msg_mod == msg {
                        break;
                    }
                }
            }
        }
    }
}
