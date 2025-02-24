use flarch::start_logging;
use flmodules::{timer::TimerMessage, Modules};
mod helpers;
use helpers::*;

async fn ping_n(nbr_nodes: usize) -> Result<(), NetworkError> {
    let mut net = NetworkSimul::new();
    log::info!("Creating {nbr_nodes} nodes");
    net.add_nodes(Modules::RAND | Modules::PING, nbr_nodes)
        .await?;
    net.process(5).await;
    for node_timer in net.nodes.values_mut() {
        node_timer.node.timer.broker.emit_msg_out(TimerMessage::Second)?;
    }

    for node_timer in net.nodes.values_mut() {
        assert_eq!(
            0,
            node_timer.node.ping.as_ref().unwrap().storage.borrow().failed.len()
        );
        assert_eq!(
            node_timer
                .node
                .random
                .as_ref()
                .unwrap()
                .storage
                .borrow()
                .connected
                .0
                .len(),
            node_timer.node.ping.as_ref().unwrap().storage.borrow().stats.len()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ping() -> Result<(), NetworkError> {
        start_logging();

        for n in &[2, 3, 4, 5, 10, 20, 50, 100] {
            ping_n(*n).await?;
        }
        Ok(())
    }
}
