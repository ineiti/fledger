use flmodules::timer::TimerMessage;
use flnode::node_data::Brokers;
use flutils::{broker::Broker, start_logging};
mod helpers;
use helpers::*;

#[tokio::test]
async fn ping() -> Result<(), NetworkError> {
    start_logging();

    for n in &[2, 3, 4, 5, 10, 20, 50, 100] {
        ping_n(*n).await?;
    }
    Ok(())
}

async fn ping_n(nbr_nodes: usize) -> Result<(), NetworkError> {
    let mut net = Network::new();
    log::info!("Creating {nbr_nodes} nodes");
    let mut timer = Broker::new();
    net.add_nodes(Brokers::ENABLE_RAND | Brokers::ENABLE_PING, nbr_nodes).await?;
    for node in net.nodes.values_mut() {
        node.node_data.add_timer(timer.clone()).await;
    }
    net.process(5).await;
    timer.emit_msg(TimerMessage::Second).await?;

    for node in net.nodes.values_mut() {
        node.node_data.update();
        assert_eq!(0, node.node_data.ping.storage.failed.len());
        assert_eq!(
            node.node_data.random.storage.connected.0.len(),
            node.node_data.ping.storage.stats.len()
        );
    }

    Ok(())
}
