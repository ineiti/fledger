use flarch::{
    broker::BrokerError,
    data_storage::{DataStorage, DataStorageTemp},
    start_logging_filter_level,
    tasks::wait_ms,
    web_rtc::{
        connection::ConnectionConfig, web_socket_server::WebSocketServer, websocket::WSSError,
    },
};
use flmodules::network::{network_start, signal::SignalServer, NetworkSetupError};
use flnode::node::{Node, NodeError};
use thiserror::Error;

#[derive(Debug, Error)]
enum TestError {
    #[error(transparent)]
    Node(#[from] NodeError),
    #[error(transparent)]
    WSServer(#[from] WSSError),
    #[error(transparent)]
    Broker(#[from] BrokerError),
    #[error(transparent)]
    Network(#[from] NetworkSetupError),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    start_logging_filter_level(vec!["signal", "fl"], log::LevelFilter::Info);

    let wss = WebSocketServer::new(8765).await?;
    let mut signal_server = SignalServer::new(wss, 2).await?;
    let (msgs_signal, _) = signal_server.get_tap_out_sync().await?;
    log::info!("Started listening on port 8765");

    let node1 = create_node().await?;
    log::info!("Node1: {}", node1.node_config.info.get_id());
    let node2 = create_node().await?;
    log::info!("Node2: {}", node2.node_config.info.get_id());

    for i in 0..5 {
        log::info!("Running step {i}");
        log::info!(
            "Node 1: {:?}",
            node1.ping.as_ref().unwrap().storage.borrow().stats
        );

        log::info!(
            "Node 2: {:?}",
            node2.ping.as_ref().unwrap().storage.borrow().stats
        );

        for msg in msgs_signal.try_iter() {
            log::debug!("Signal: {msg:?}");
        }

        wait_ms(1000).await;
    }

    let ping1 = node1
        .ping
        .as_ref()
        .unwrap()
        .storage
        .borrow()
        .stats
        .get(&node2.node_config.info.get_id())
        .unwrap()
        .clone();
    assert_eq!(1, ping1.tx);
    assert_eq!(1, ping1.rx);

    let ping2 = node2
        .ping
        .as_ref()
        .unwrap()
        .storage
        .borrow()
        .stats
        .get(&node1.node_config.info.get_id())
        .unwrap()
        .clone();
    assert_eq!(1, ping2.tx);
    assert_eq!(1, ping2.rx);

    log::info!("All seems well and ping messages have been passed");

    Ok(())
}

async fn create_node() -> anyhow::Result<Node> {
    let storage = DataStorageTemp::new();
    let node_config = Node::get_config(storage.clone_box())?;
    let network = network_start(
        node_config.clone(),
        ConnectionConfig::from_signal("ws://localhost:8765"),
    )
    .await?;
    Ok(Node::start(Box::new(storage), node_config, network.broker).await?)
}
