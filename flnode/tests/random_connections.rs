use flarch::start_logging;
mod helpers;
use flmodules::Modules;
use helpers::*;

async fn connect_nodes_n(nbr_nodes: usize) -> anyhow::Result<()> {
    let mut net = NetworkSimul::new();
    log::info!("Creating {nbr_nodes} nodes");
    net.add_nodes(Modules::RAND, nbr_nodes).await?;
    net.process(5).await;
    log::info!("Sent a total of {} messages", net.messages);

    log::info!("Verify for full connection of all nodes");
    let mut nodes_to_visit = vec![*net.nodes.keys().next().unwrap()];
    let mut nodes_visited = vec![];

    while !nodes_to_visit.is_empty() {
        let id = nodes_to_visit.pop().unwrap();
        if nodes_visited.contains(&id) {
            continue;
        }
        nodes_visited.push(id);
        let node = net.nodes.get_mut(&id).unwrap();
        let nd = node.node.random.as_ref().unwrap().stats.clone();
        for n in &nd.borrow().connected.0 {
            if nodes_to_visit.contains(&n.id) || nodes_visited.contains(&n.id) {
                continue;
            }
            nodes_to_visit.push(n.id);
        }
    }
    assert_eq!(nbr_nodes, nodes_visited.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connect_nodes() -> anyhow::Result<()> {
        start_logging();

        for n in &[2, 3, 4, 5, 10, 20, 50, 100] {
            connect_nodes_n(*n).await?;
        }
        Ok(())
    }
}
