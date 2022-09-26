use flarch::start_logging;
use flmodules::{broker::Broker, timer::TimerMessage};
mod helpers;
use flnode::node::Brokers;
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
    net.add_nodes(Brokers::ENABLE_RAND | Brokers::ENABLE_PING, nbr_nodes)
        .await?;
    for node_timer in net.nodes.values_mut() {
        node_timer.node.add_timer(timer.clone()).await;
    }
    net.process(5).await;
    timer.emit_msg(TimerMessage::Second).await?;

    for node_timer in net.nodes.values_mut() {
        node_timer.node.update();
        assert_eq!(0, node_timer.node.ping.as_ref().unwrap().storage.failed.len());
        assert_eq!(
            node_timer.node.random.as_ref().unwrap().storage.connected.0.len(),
            node_timer.node.ping.as_ref().unwrap().storage.stats.len()
        );
    }

    Ok(())
}
