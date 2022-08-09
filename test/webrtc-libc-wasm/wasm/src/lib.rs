use std::sync::mpsc::channel;

use thiserror::Error;
use wasm_bindgen::prelude::*;

use flnet::{
    network::{NetCall,NetworkError},
    network_start,
    NetworkSetupError,
    config::{NodeConfig, NodeInfo},
    network::{NetReply, NetworkMessage},
};
use flarch::{tasks::wait_ms};
use flmodules::{broker::Subsystem};

const URL: &str = "ws://localhost:8765";

#[derive(Debug, Error)]
enum StartError {
    #[error(transparent)]
    NetworkSetup(#[from] NetworkSetupError),
    #[error(transparent)]
    Network(#[from] NetworkError),
    #[error(transparent)]
    Broker(#[from] flmodules::broker::BrokerError),
}

async fn run_app() -> Result<(), StartError> {
    log::info!("Starting app");

    let nc = NodeConfig::new();
    let mut net = network_start(nc.clone(), URL).await?;
    let (tx, rx) = channel();
    net.add_subsystem(Subsystem::Tap(tx)).await?;
    let mut i: i32 = 0;
    loop {
        i += 1;

        if i % 10 == 2 {
            log::info!("Waiting - {}", i / 10);
        }
        while let Ok(msg) = rx.try_recv() {
            if let NetworkMessage::Reply(reply) = msg {
                match reply {
                    NetReply::RcvNodeMessage(node) => {
                        log::info!("Got node message: {:?}", node)
                    }
                    NetReply::RcvWSUpdateList(list) => {
                        let other: Vec<NodeInfo> = list
                            .iter()
                            .filter(|n| n.get_id() != nc.info.get_id())
                            .cloned()
                            .collect();
                        log::info!("Got list: {:?}", other);
                        if other.len() > 0 {
                            net.emit_msg(
                                NetReply::RcvNodeMessage((
                                    other.get(0).unwrap().get_id(),
                                    "Hello from Rust wasm".to_string(),
                                )).into()
                            )
                            .await?;
                        }
                    }
                    _ => log::debug!("Got other message: {:?}", reply),
                }
            }
        }
        net.emit_msg(NetworkMessage::Call(NetCall::SendWSStats(vec![])))
            .await?;
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
