use flnet::network::{NetCall,NetworkError};
use flnet_wasm::{web_rtc::WebRTCConnection,web_socket_client::{WSClientError,WebSocketClient}};
use std::sync::mpsc::channel;

use thiserror::Error;
use wasm_bindgen::prelude::*;

use flnet::{
    config::{NodeConfig, NodeInfo},
    network::{NetReply, Network, NetworkMessage},
    signal::websocket::WSError,
};
use flarch::{broker::Subsystem, arch::wait_ms};

const URL: &str = "ws://localhost:8765";

#[derive(Debug, Error)]
enum StartError {
    #[error(transparent)]
    WS(#[from] WSError),
    #[error(transparent)]
    Network(#[from] NetworkError),
    #[error(transparent)]
    Broker(#[from] flmodules::broker::BrokerError),
    #[error(transparent)]
    Client(#[from] WSClientError),
}

async fn run_app() -> Result<(), StartError> {
    log::info!("Starting app");

    let nc = NodeConfig::new();
    let ws = WebSocketClient::connect(URL).await?;
    let mut net = Network::start(
        nc.clone(),
        ws,
        Box::new(|| Box::new(Box::pin(WebRTCConnection::new_box()))),
    )
    .await?;
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
                            .filter(|n| n.get_id() != nc.our_node.get_id())
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
