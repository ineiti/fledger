use flarch::start_logging;
mod helpers;
use flnode::node::Brokers;
use helpers::*;

#[tokio::test]
async fn connect_nodes() -> Result<(), NetworkError> {
    start_logging();

    for n in &[2, 3, 4, 5, 10, 20, 50, 100] {
        connect_nodes_n(*n).await?;
    }
    Ok(())
}

async fn connect_nodes_n(nbr_nodes: usize) -> Result<(), NetworkError> {
    let mut net = Network::new();
    log::info!("Creating {nbr_nodes} nodes");
    net.add_nodes(Brokers::ENABLE_RAND, nbr_nodes).await?;
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
        node.node.update();
        let nd = node.node.random.as_ref().unwrap().storage.clone();
        for n in nd.connected.0 {
            if nodes_to_visit.contains(&n.id) || nodes_visited.contains(&n.id) {
                continue;
            }
            nodes_to_visit.push(n.id);
        }
    }
    assert_eq!(nbr_nodes, nodes_visited.len());
    Ok(())
}
