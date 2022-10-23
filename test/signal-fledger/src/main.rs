use flarch::{
    data_storage::{DataStorage, DataStorageTemp},
    start_logging_filter_level, wait_ms,
};
use flnet::{
    broker::BrokerError, network_broker_start, signal::SignalServer, web_socket_server::WebSocketServer,
    websocket::WSSError, NetworkSetupError,
};
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
async fn main() -> Result<(), TestError> {
    start_logging_filter_level(vec!["signal", "fl"], log::LevelFilter::Info);
 
    let wss = WebSocketServer::new(8765).await?;
    let mut signal_server = SignalServer::new(wss, 2).await?;
    let (msgs_signal, _) = signal_server.get_tap().await?;
    log::info!("Started listening on port 8765");

    let mut node1 = create_node().await?;
    log::info!("Node1: {}", node1.node_config.info.get_id());
    let mut node2 = create_node().await?;
    log::info!("Node2: {}", node2.node_config.info.get_id());

    for i in 0..5 {
        log::info!("Running step {i}");
        node1.process().await?;
        log::info!("Node 1: {:?}", node1.ping.as_ref().unwrap().storage.stats);

        node2.process().await?;
        log::info!("Node 2: {:?}", node2.ping.as_ref().unwrap().storage.stats);

        for msg in msgs_signal.try_iter() {
            log::debug!("Signal: {msg:?}");
        }

        wait_ms(1000).await;
    }

    let ping1 = node1.ping.as_ref().unwrap().storage.stats.get(&node2.node_config.info.get_id()).unwrap();
    assert_eq!(1, ping1.tx);
    assert_eq!(1, ping1.rx);

    let ping2 = node2.ping.as_ref().unwrap().storage.stats.get(&node1.node_config.info.get_id()).unwrap();
    assert_eq!(1, ping2.tx);
    assert_eq!(1, ping2.rx);

    log::info!("All seems well and ping messages have been passed");

    Ok(())
}

async fn create_node() -> Result<Node, TestError> {
    let storage = DataStorageTemp::new();
    let node_config = Node::get_config(storage.clone())?;
    let network = network_broker_start(node_config.clone(), "ws://localhost:8765").await?;
    Node::start(Box::new(storage), node_config, network)
        .await
        .map_err(|e| e.into())
}
