use wasm_bindgen::prelude::*;

use flarch::{
    broker::Destination,
    tasks::wait_ms,
    web_rtc::connection::ConnectionConfig,
};
use flmodules::{
    network::{
        broker::{NetworkIn, NetworkOut},
        network_start,
    },
    timer::Timer,
};
use flmodules::{
    nodeconfig::{NodeConfig, NodeInfo},
    router::messages::NetworkWrapper,
};

const URL: &str = "ws://127.0.0.1:8765";
const MODULE_NAME: &str = "LIBC_WASM";

async fn run_app() -> anyhow::Result<()> {
    log::info!("Starting app");

    let nc = NodeConfig::new();
    let mut net = network_start(
        nc.clone(),
        ConnectionConfig::from_signal(URL),
        &mut Timer::start().await?,
    )
    .await?
    .broker;
    let (rx, tap_indx) = net.get_tap_out_sync().await?;
    let mut i: i32 = 0;
    loop {
        i += 1;

        if i % 10 == 2 {
            log::info!("Waiting - {}", i / 10);
        }
        while let Ok(msg) = rx.try_recv() {
            match msg {
                NetworkOut::MessageFromNode(id, msg_net) => {
                    log::info!("Got node message: {} / {:?}", id, msg_net);
                    net.remove_subsystem(tap_indx).await?;
                    return Ok(());
                }
                NetworkOut::NodeListFromWS(list) => {
                    let other: Vec<NodeInfo> = list
                        .iter()
                        .filter(|n| n.get_id() != nc.info.get_id())
                        .cloned()
                        .collect();
                    log::info!("Got list: {:?}", other);
                    if other.len() > 0 {
                        net.emit_msg_in_dest(
                            Destination::NoTap,
                            NetworkIn::MessageToNode(
                                other.get(0).unwrap().get_id(),
                                NetworkWrapper::wrap_yaml(
                                    MODULE_NAME,
                                    &"Hello from Rust wasm".to_string(),
                                )
                                .expect("Creating NetworkWrapper"),
                            )
                            .into(),
                        )?;
                    }
                }
                _ => log::debug!("Got other message: {:?}", msg),
            }
        }
        net.emit_msg_in(NetworkIn::WSUpdateListRequest)?;
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
