use std::str::FromStr;

use flarch::nodeids::{NodeID, U256};

use super::kademlia::{KNode, Kademlia};

pub fn rnd_node_depth(root: &NodeID, depth: usize) -> NodeID {
    loop {
        let node = NodeID::rnd();
        let nd = KNode::get_depth(root, node);
        if nd == depth {
            return node;
        }
    }
}

pub fn rnd_nodes_depth(root: &NodeID, depth: usize, nbr: usize) -> Vec<NodeID> {
    (0..nbr).map(|_| rnd_node_depth(root, depth)).collect()
}

pub fn distance_str(root: &NodeID, node_str: &str) -> usize {
    let node = U256::from_str(node_str).expect("NodeID init");
    KNode::get_depth(root, node)
}

pub fn kademlia_add_nodes(kademlia: &mut Kademlia, depth: usize, nbr: usize) {
    kademlia.add_nodes_active(rnd_nodes_depth(&kademlia.root, depth, nbr));
}

pub fn kademlia_add_nodes_cache(kademlia: &mut Kademlia, depth: usize, nbr: usize) {
    kademlia.add_nodes(rnd_nodes_depth(&kademlia.root, depth, nbr));
}

