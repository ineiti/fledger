use std::sync::mpsc::RecvError;

use thiserror::Error;
use wasm_bindgen::prelude::*;

use flarch::{broker::{Destination, BrokerError}, tasks::wait_ms, web_rtc::connection::ConnectionConfig};
use flmodules::nodeconfig::{NodeConfig, NodeInfo};
use flnet::{
    network::{NetCall, NetReply, NetworkError, NetworkMessage},
    network_broker_start, NetworkSetupError,
};

const URL: &str = "ws://127.0.0.1:8765";

#[derive(Debug, Error)]
enum StartError {
    #[error(transparent)]
    NetworkSetup(#[from] NetworkSetupError),
    #[error(transparent)]
    Network(#[from] NetworkError),
    #[error(transparent)]
    Broker(#[from] BrokerError),
    #[error(transparent)]
    Receive(#[from] RecvError),
}

async fn run_app() -> Result<(), StartError> {
    log::info!("Starting app");

    let nc = NodeConfig::new();
    let mut net = network_broker_start(nc.clone(), ConnectionConfig::from_signal(URL)).await?;
    let (rx, tap_indx) = net.get_tap_sync().await?;
    let mut i: i32 = 0;
    loop {
        i += 1;

        if i % 10 == 2 {
            log::info!("Waiting - {}", i / 10);
        }
        while let Ok(msg) = rx.try_recv() {
            if let NetworkMessage::Reply(reply) = msg {
                match reply {
                    NetReply::RcvNodeMessage(id, msg_net) => {
                        log::info!("Got node message: {} / {:?}", id, msg_net);
                        net.remove_subsystem(tap_indx).await?;
                        return Ok(());
                    }
                    NetReply::RcvWSUpdateList(list) => {
                        let other: Vec<NodeInfo> = list
                            .iter()
                            .filter(|n| n.get_id() != nc.info.get_id())
                            .cloned()
                            .collect();
                        log::info!("Got list: {:?}", other);
                        if other.len() > 0 {
                            net.emit_msg_dest(
                                Destination::NoTap,
                                NetCall::SendNodeMessage(
                                    other.get(0).unwrap().get_id(),
                                    "Hello from Rust wasm".to_string(),
                                )
                                .into(),
                            )?;
                        }
                    }
                    _ => log::debug!("Got other message: {:?}", reply),
                }
            }
        }
        net.emit_msg(NetworkMessage::Call(NetCall::SendWSUpdateListRequest))?;
        wait_ms(1000).await;
    }
}

#[wasm_bindgen(start)]
pub async fn main() {
    console_error_panic_hook::set_once();

    wasm_logger::init(wasm_logger::Config::default());

    if let Err(e) = run_app().await {
        log::error!("Error: {:?}", e);
    }
}
