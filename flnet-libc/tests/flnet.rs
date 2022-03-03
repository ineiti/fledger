use std::sync::mpsc::channel;

use flnet::{config::NodeConfig, network::{Network, NetworkMessage}, signal::dummy::WebSocketSimul};
use flnet_libc::web_rtc_setup::WebRTCConnectionSetupLibc;
use flutils::{time::wait_ms, broker::Subsystem};
use thiserror::Error;

#[derive(Debug, Error)]
enum TestError {
    #[error(transparent)]
    Network(#[from] flnet::network::NetworkError),
    #[error(transparent)]
    Broker(#[from] flutils::broker::BrokerError),
}

#[tokio::test(flavor = "multi_thread")]
async fn test_nodes() -> Result<(), TestError> {
    let _ = env_logger::Builder::new()
        .filter_level(log::LevelFilter::Debug)
        .parse_env("RUST_LOG")
        .try_init();

    let nc1 = NodeConfig::new();
    let nc2 = NodeConfig::new();
    let mut wss = WebSocketSimul::new();
    let _ws = wss.new_server();

    let (_id1, conn1) = wss.new_connection();
    let mut net1 = Network::start(
        nc1.clone(),
        Box::new(conn1),
        Box::new(WebRTCConnectionSetupLibc::new_box),
    );
    let (_id2, conn2) = wss.new_connection();
    let mut net2 = Network::start(
        nc2.clone(),
        Box::new(conn2),
        Box::new(WebRTCConnectionSetupLibc::new_box),
    );
    let (tap_tx, tap_rx) = channel();
    net2.add_subsystem(Subsystem::Tap(tap_tx))?;

    net1.emit_msg(NetworkMessage{
        id: nc2.our_node.get_id(),
        msg: "ping".to_string(),
    }.to_net())?;

    wait_ms(1000).await;
    wss.process();
    wait_ms(1000).await;
    wss.process();
    wait_ms(1000).await;
    wss.process();

    let msg = tap_rx.try_recv().unwrap();
    log::debug!("{:?}", msg);

    Ok(())
}
