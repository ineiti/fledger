mod helpers;
use helpers::*;

#[test]
fn connect_nodes() {
    let _ = env_logger::init();
    let nbr_nodes = 1000;

    let mut net = Network::new();
    log::info!("Creating {nbr_nodes} nodes");
    net.add_nodes(nbr_nodes);
    log::info!("Processing 5 times");
    net.process(5);

    log::info!("Verify for full connection of all nodes");
    let mut nodes_to_visit = vec![*net.nodes.keys().next().unwrap()];
    let mut nodes_visited = vec![];

    while !nodes_to_visit.is_empty() {
        let node = nodes_to_visit.pop().unwrap();
        if nodes_visited.contains(&node) {
            continue;
        }
        nodes_visited.push(node);
        let nd = net.nodes.get(&node).unwrap().node_data.lock().unwrap();
        for n in nd.random_connections.connected().0 {
            if nodes_to_visit.contains(&n) || nodes_visited.contains(&n) {
                continue;
            }
            nodes_to_visit.push(n);
        }
    }
    assert_eq!(nbr_nodes, nodes_visited.len());
}
