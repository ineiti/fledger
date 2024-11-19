use core::fmt;
use std::fmt::Debug;

use flarch::nodeids::{NodeID, NodeIDs};
use serde::de;

pub struct Kademlia {
    // All buckets starting with a distance of 0..depth-1 0s followed by at least
    // one 1.
    buckets: Vec<KBucket>,
    // The home of the root bucket.
    root_bucket: KBucket,
    // The root node of this Kademlia instance
    root: NodeID,
    // Maximum number of nodes per bucket
    k: usize,
}

impl Kademlia {
    pub fn new(k: usize, root: NodeID) -> Self {
        Self {
            buckets: vec![],
            root_bucket: KBucket::new(k),
            root,
            k,
        }
    }

    pub fn add_node(&mut self, id: NodeID) {
        let node = KNode::new(&self.root, id);
        if let Some(bucket) = self.buckets.get_mut(node.depth) {
            bucket.add_node(node);
        } else {
            self.root_bucket.add_node(node);
            if self.root_bucket.status() == BucketStatus::Overflowing {
                self.rebalance_overflow();
            }
        };
    }

    pub fn remove_node(&mut self, id: &NodeID) {
        let node = KNode::new(&self.root, *id);
        if let Some(bucket) = self.buckets.get_mut(node.depth) {
            bucket.remove_node(id);
        } else {
            self.root_bucket.remove_node(id);
            if self.root_bucket.status() == BucketStatus::Wanting {
                self.rebalance_wanting();
            }
        };
    }

    pub fn nearest_nodes(&mut self, id: &NodeID) -> Vec<NodeID> {
        let kn = KNode::new(&self.root, *id);

        // Start with potential nodes from the root-bucket.
        for depth in (self.buckets.len()..=kn.depth).rev() {
            let nodes = self.root_bucket.get_active_ids(depth);
            if nodes.len() > 0 {
                return nodes;
            }
        }

        // Then search nodes which are at the same depth or further away.
        for depth in (0..=kn.depth).rev() {
            if let Some(nodes) = self.get_active_bucket_ids(depth) {
                return nodes;
            }
        }

        // Then try nodes closer - this will probably fail, but might as well try it.
        for depth in kn.depth + 1..self.buckets.len() {
            if let Some(nodes) = self.get_active_bucket_ids(depth) {
                return nodes;
            }
        }

        // OK, nothing found
        vec![]
    }

    pub fn ping_answer(&mut self, _node: NodeID) {}

    pub fn tick(&mut self) -> Vec<NodeID> {
        vec![]
    }

    // The root bucket overflows - split it until it has between k and 2k
    // nodes.
    fn rebalance_overflow(&mut self) {
        while self.root_bucket.status() == BucketStatus::Overflowing {
            let mut bucket = KBucket::new(self.k);
            bucket.add_nodes(
                self.root_bucket
                    .remove_all_nodes_at_depth(self.buckets.len()),
            );
            self.buckets.push(bucket);
            self.root_bucket.cache_to_active();
        }
    }

    // The root bucket is wanting - merge with non-empty upper buckets until
    // it's safe again.
    fn rebalance_wanting(&mut self) {
        while self.root_bucket.status() == BucketStatus::Wanting {
            if let Some(bucket) = self.buckets.pop() {
                self.root_bucket.add_nodes(bucket.active);
                self.root_bucket.add_nodes(bucket.cache);
                self.root_bucket.cache_to_active();
            } else {
                return;
            }
        }
    }

    fn get_active_bucket_ids(&self, depth: usize) -> Option<Vec<NodeID>> {
        if let Some(bucket) = self.buckets.get(depth) {
            if bucket.active.len() > 0 {
                return Some(bucket.active.iter().map(|kn| kn.node).collect());
            }
        }
        None
    }
}

impl fmt::Display for Kademlia {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Kademlia root: {}\n", self.root))?;
        f.write_fmt(format_args!("k: {}\n", self.k))?;
        for (index, bucket) in self.buckets.iter().enumerate() {
            f.write_fmt(format_args!("Bucket[{index}] = {}\n", bucket))?;
        }
        f.write_fmt(format_args!("Root-bucket: {}", self.root_bucket))
    }
}

struct KBucket {
    active: Vec<KNode>,
    cache: Vec<KNode>,
    k: usize,
}

impl KBucket {
    fn new(k: usize) -> Self {
        Self {
            active: vec![],
            cache: vec![],
            k,
        }
    }

    fn add_node(&mut self, node: KNode) {
        if self.active.len() < self.k {
            self.active.push(node);
        } else {
            self.cache.push(node);
        }
    }

    fn add_nodes(&mut self, nodes: Vec<KNode>) {
        for node in nodes.into_iter() {
            self.add_node(node);
        }
    }

    fn remove_node(&mut self, id: &NodeID) {
        if let Some(pos) = self.active.iter().position(|kn| &kn.node == id) {
            self.active.remove(pos);
            self.cache_to_active();
        } else if let Some(pos) = self.cache.iter().position(|kn| &kn.node == id) {
            self.cache.remove(pos);
        }
    }

    fn cache_to_active(&mut self) {
        while self.active.len() < self.k && self.cache.len() > 0 {
            self.active.push(self.cache.pop().unwrap());
        }
    }

    fn remove_all_nodes_at_depth(&mut self, depth: usize) -> Vec<KNode> {
        let mut nodes: Vec<KNode> = self
            .active
            .iter()
            .chain(self.cache.iter())
            .filter(|n| n.depth == depth)
            .cloned()
            .collect();
        let left = self.active.len() + self.cache.len() - nodes.len();
        if left < self.k {
            nodes.splice(left..self.k, vec![]);
        }
        self.active.retain(|n| !nodes.contains(n));
        self.cache.retain(|n| !nodes.contains(n));
        nodes
    }

    fn get_active_ids(&mut self, depth: usize) -> Vec<NodeID> {
        self.active
            .iter()
            .filter_map(|kn| (kn.depth == depth).then(|| kn.node))
            .collect()
    }

    // Rust range expressions:
    // 0..k nodes: wanting
    // k..=2k nodes: stable
    // 2k+1.. nodes: overflowing
    fn status(&mut self) -> BucketStatus {
        let nodes = self.active.len() + self.cache.len();
        if nodes > 2 * self.k {
            BucketStatus::Overflowing
        } else if nodes < self.k {
            BucketStatus::Wanting
        } else {
            BucketStatus::Stable
        }
    }
}

impl fmt::Display for KBucket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "Active: {} :: Cache: {}",
            NodeIDs::from(
                self.active
                    .iter()
                    .map(|kn| kn.node)
                    .collect::<Vec<NodeID>>()
            ),
            NodeIDs::from(self.cache.iter().map(|kn| kn.node).collect::<Vec<NodeID>>()),
        ))?;
        Ok(())
    }
}

#[derive(PartialEq)]
enum BucketStatus {
    // #nodes > 2k
    Overflowing,
    // k < #nodes <= 2k
    Stable,
    // #nodes < k
    Wanting,
}

#[derive(Debug, Clone)]
struct KNode {
    node: NodeID,
    depth: usize,
    distance: Vec<Bit>,
    state: NodeState,
}

impl KNode {
    pub fn new(root: &NodeID, node: NodeID) -> Self {
        let distance = Self::distance(root, node);
        Self {
            node,
            state: NodeState::Active,
            depth: Self::get_depth(&distance),
            distance,
        }
    }

    fn distance(root: &NodeID, node: NodeID) -> Vec<Bit> {
        let mut node_bytes = node.to_bytes().to_vec();
        root.to_bytes()
            .iter()
            .map(|c| (c ^ node_bytes.remove(0)) as u32)
            .flat_map(|mut x| {
                (0..=7).map(move |_| {
                    x *= 2;
                    if x & 0x100 > 0 {
                        Bit::One
                    } else {
                        Bit::Zero
                    }
                })
            })
            .collect()
    }

    fn get_depth(distance: &[Bit]) -> usize {
        for depth in 0..distance.len() {
            if distance.get(depth) == Some(&Bit::One) {
                return depth;
            }
        }
        return distance.len();
    }

    fn set_state(&mut self, state: NodeState) {
        self.state = state;
    }
}

impl PartialEq for KNode {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node
    }
}

#[derive(Debug, PartialEq, Clone)]
enum Bit {
    Zero,
    One,
}

#[derive(Debug, Clone)]
enum NodeState {
    // actively used in the k-bucket
    Active,
    // node has been pinged this many times
    Ping(usize),
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use flarch::{nodeids::U256, start_logging_filter_level};

    use super::*;

    fn rnd_node_depth(root: &NodeID, depth: usize) -> NodeID {
        loop {
            let node = NodeID::rnd();
            let dist = KNode::distance(root, node);
            let nd = KNode::get_depth(&dist);
            if nd == depth {
                //log::info!("{}-{}: {:?}", depth, node, dist);
                return node;
            }
        }
    }

    fn rnd_nodes_depth(root: &NodeID, depth: usize, nbr: usize) -> Vec<NodeID> {
        (0..nbr).map(|_| rnd_node_depth(root, depth)).collect()
    }

    fn distance_str(root: &NodeID, node_str: &str) -> usize {
        let node = U256::from_str(node_str).expect("NodeID init");
        KNode::get_depth(&KNode::distance(root, node))
    }

    #[test]
    fn test_distance() {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let root = U256::from_str("00").expect("NodeID init");
        assert_eq!(256, KNode::get_depth(&KNode::distance(&root, root)));
        assert_eq!(0, distance_str(&root, "80"));
        assert_eq!(1, distance_str(&root, "40"));
        assert_eq!(2, distance_str(&root, "20"));
        assert_eq!(3, distance_str(&root, "10"));
        assert_eq!(4, distance_str(&root, "08"));
        assert_eq!(5, distance_str(&root, "04"));
        assert_eq!(6, distance_str(&root, "02"));
        assert_eq!(7, distance_str(&root, "01"));
        assert_eq!(8, distance_str(&root, "0080"));
    }

    fn kademlia_add_nodes(kademlia: &mut Kademlia, depth: usize, nbr: usize) {
        for node in rnd_nodes_depth(&kademlia.root, depth, nbr) {
            kademlia.add_node(node);
        }
    }

    #[test]
    fn test_rnd_node_depth() {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let node = U256::from_str("00").expect("NodeID init");
        log::info!("{}", node);
        for i in 0..16 {
            log::info!("{} - {}", i, rnd_node_depth(&node, i));
        }
    }

    fn kademlia_test(kademlia: &Kademlia, buckets: usize, root_active: usize, root_cache: usize) {
        assert_eq!(buckets, kademlia.buckets.len(), "Wrong number of buckets");
        assert_eq!(
            root_active,
            kademlia.root_bucket.active.len(),
            "Active KBucket number mismatch"
        );
        assert_eq!(
            root_cache,
            kademlia.root_bucket.cache.len(),
            "Cache KBucket number mismatch"
        );
    }

    #[test]
    fn test_adding_nodes() {
        let root = U256::from_str("00").expect("NodeID init");
        let k = 2;
        let mut kademlia = Kademlia::new(k, root);
        kademlia_add_nodes(&mut kademlia, 0, k);
        kademlia_test(&kademlia, 0, k, 0);

        kademlia_add_nodes(&mut kademlia, 2, k);
        kademlia_test(&kademlia, 0, k, k);

        kademlia_add_nodes(&mut kademlia, 3, 1);
        kademlia_test(&kademlia, 1, k, k - 1);

        kademlia_add_nodes(&mut kademlia, 4, k);
        kademlia_test(&kademlia, 3, k, k - 1);

        // TODO: make this work correctly - what is the good thing to do here?
        let mut kademlia = Kademlia::new(k, root);
        kademlia_add_nodes(&mut kademlia, 2, 3 * k);
        kademlia_test(&kademlia, 3, k, 0);
    }

    #[test]
    fn test_removing_nodes() {
        let root = U256::from_str("00").expect("NodeID init");
        let k = 2;
        let mut kademlia = Kademlia::new(k, root);
        kademlia_add_nodes(&mut kademlia, 0, k);
        kademlia_add_nodes(&mut kademlia, 2, k);
        kademlia_add_nodes(&mut kademlia, 3, 1);
        kademlia_add_nodes(&mut kademlia, 4, k);

        kademlia_test(&kademlia, 3, k, k - 1);
        let id = kademlia.root_bucket.active.get(0).unwrap().node;
        kademlia.remove_node(&id);
        kademlia_test(&kademlia, 3, k, 0);
        let id = kademlia.root_bucket.active.get(0).unwrap().node;
        kademlia.remove_node(&id);
        kademlia_test(&kademlia, 2, k, k - 1);
    }

    // Test the standard edge-cases: no nodes, only depth==0 nodes
    #[test]
    fn test_closest_nodes_simple() {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let root = U256::from_str("00").expect("NodeID init");
        let k = 3;
        let mut kademlia = Kademlia::new(k, root);

        let nodes = kademlia.nearest_nodes(&rnd_node_depth(&root, 2));
        assert_eq!(0, nodes.len());
        kademlia_add_nodes(&mut kademlia, 0, 1);
        let nodes = kademlia.nearest_nodes(&rnd_node_depth(&root, 1));
        assert_eq!(1, nodes.len());
        kademlia_add_nodes(&mut kademlia, 2, 3 * k);
        let nodes = kademlia.nearest_nodes(&rnd_node_depth(&root, 1));
        assert_eq!(1, nodes.len());
    }

    // Do some more complicated cases
    #[test]
    fn test_closest_nodes_complicated() {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let root = U256::from_str("00").expect("NodeID init");
        let k = 2;
        let mut kademlia = Kademlia::new(k, root);

        kademlia_add_nodes(&mut kademlia, 2, k);
        kademlia_add_nodes(&mut kademlia, 3, k);
        // As the depth==4 nodes are entered one-by-one, the first one
        // triggers the overflow, and the second one goes to the cache.
        // So the active KBucket of the root_bucket has a depth 3 and a
        // depth 4 inside.
        kademlia_add_nodes(&mut kademlia, 4, k);

        let tests = vec![
            (0, 2, k),
            (1, 2, k),
            (2, 2, k),
            (3, 3, 1),
            (4, 4, 1),
            (5, 4, 1),
        ];

        for (depth, returned, nbr) in tests {
            let nodes = kademlia.nearest_nodes(&rnd_node_depth(&root, depth));
            assert_eq!(
                nbr,
                nodes.len(),
                "{depth} - {returned} - {nbr} - {}\n{kademlia}",
                NodeIDs::from(nodes),
            );
            for node in nodes {
                let kn = KNode::new(&root, node);
                assert_eq!(
                    returned, kn.depth,
                    "Node {node} with depth {} in {depth} - {returned} - {nbr}",
                    kn.depth
                );
            }
        }
    }
}
