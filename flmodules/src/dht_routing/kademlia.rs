use core::fmt;
use std::fmt::Debug;

use flarch::nodeids::{NodeID, NodeIDs};
use itertools::Itertools;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Kademlia {
    // All buckets starting with a distance of 0..depth-1 0s followed by at least
    // one 1.
    buckets: Vec<KBucket>,
    // The home of the root bucket.
    root_bucket: KBucket,
    // The root node of this Kademlia instance
    pub root: NodeID,
    // Configuration for the buckets
    config: Config,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub struct Config {
    pub k: usize,
    pub ping_interval: u32,
    pub ping_timeout: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            k: 5,
            ping_interval: 10,
            ping_timeout: 20,
        }
    }
}

impl Kademlia {
    pub fn new(root: NodeID, config: Config) -> Self {
        Self {
            buckets: vec![],
            root_bucket: KBucket::new_root(config),
            root,
            config,
        }
    }

    pub fn add_node(&mut self, id: NodeID) {
        if id == self.root || self.has_node_id(&id) {
            return;
        }
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

    pub fn add_nodes(&mut self, ids: Vec<NodeID>) {
        for id in ids {
            self.add_node(id);
        }
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

    pub fn nearest_nodes(&self, id: &NodeID) -> Vec<NodeID> {
        if id == &self.root {
            return vec![];
        }

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

    /// Returns all the active nodes of the kademlia system.
    pub fn active_nodes(&self) -> Vec<NodeID> {
        self.buckets
            .iter()
            .flat_map(|b| &b.active)
            .chain(&self.root_bucket.active)
            .map(|kn| kn.id)
            .collect()
    }

    /// The system received a message from this node, so its considered
    /// active.
    pub fn node_active(&mut self, id: &NodeID) {
        if let Some(kn) = self.get_knode(id) {
            kn.active();
        }
    }

    /// Advances the clock by one tick and returns the nodes which were removed,
    /// and the nodes which need to be pinged.
    pub fn tick(&mut self) -> TickIDs {
        let mut ping_ids = TickIDs::default();
        for bucket in self.buckets.iter_mut() {
            ping_ids.extend(bucket.tick());
        }
        ping_ids.extend(self.root_bucket.tick());
        ping_ids
    }

    fn get_knode(&mut self, id: &NodeID) -> Option<&mut KNode> {
        let kn = KNode::new(&self.root, *id);
        if let Some(bucket) = self.buckets.get_mut(kn.depth) {
            if let Some(kn) = bucket.get_knode(id) {
                return Some(kn);
            }
        }
        self.root_bucket.get_knode(id)
    }

    // The root bucket overflows - split it until it has between k and 2k
    // nodes.
    fn rebalance_overflow(&mut self) {
        while self.root_bucket.status() == BucketStatus::Overflowing {
            let mut bucket = KBucket::new(self.config);
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
                return Some(bucket.active.iter().map(|kn| kn.id).collect());
            }
        }
        None
    }

    fn has_node_id(&self, id: &NodeID) -> bool {
        self.buckets
            .iter()
            .chain(std::iter::once(&self.root_bucket))
            .flat_map(|b| b.active.iter().chain(b.cache.iter()))
            .any(|kn| &kn.id == id)
    }
}

impl fmt::Display for Kademlia {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Kademlia root: {}\n", self.root))?;
        f.write_fmt(format_args!("k: {}\n", self.config.k))?;
        for (index, bucket) in self.buckets.iter().enumerate() {
            f.write_fmt(format_args!("Bucket[{index}] = {}\n", bucket))?;
        }
        f.write_fmt(format_args!("Root-bucket: {}", self.root_bucket))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
struct KBucket {
    active: Vec<KNode>,
    cache: Vec<KNode>,
    config: Config,
    is_root: bool,
}

impl KBucket {
    fn new(config: Config) -> Self {
        Self {
            active: vec![],
            cache: vec![],
            config,
            is_root: false,
        }
    }

    fn new_root(config: Config) -> Self {
        Self {
            active: vec![],
            cache: vec![],
            config,
            is_root: true,
        }
    }

    fn add_node(&mut self, node: KNode) {
        if self.is_root {
            self.active.push(node);
        } else if self.active.len() < self.config.k {
            self.active.push(node);
        } else if self.cache.len() < self.config.k {
            self.cache.push(node);
        }
    }

    fn add_nodes(&mut self, nodes: Vec<KNode>) {
        for node in nodes.into_iter() {
            self.add_node(node);
        }
    }

    fn remove_node(&mut self, id: &NodeID) {
        if let Some(pos) = self.active.iter().position(|kn| &kn.id == id) {
            self.active.remove(pos);
            self.cache_to_active();
        } else if let Some(pos) = self.cache.iter().position(|kn| &kn.id == id) {
            self.cache.remove(pos);
        }
    }

    fn cache_to_active(&mut self) {
        while self.active.len() < self.config.k {
            match self.cache.pop() {
                Some(mut bucket) => {
                    bucket.reset();
                    self.active.push(bucket);
                }
                None => break,
            }
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
        if left < self.config.k {
            nodes.splice(left..self.config.k, vec![]);
        }
        self.active.retain(|n| !nodes.contains(n));
        self.cache.retain(|n| !nodes.contains(n));
        nodes
    }

    fn get_active_ids(&self, depth: usize) -> Vec<NodeID> {
        self.active
            .iter()
            .filter_map(|kn| (kn.depth == depth).then(|| kn.id))
            .collect()
    }

    // Rust range expressions:
    // 0..k nodes: wanting
    // k..=2k nodes: stable
    // 2k+1.. nodes: overflowing
    fn status(&mut self) -> BucketStatus {
        let nodes = self.active.len() + self.cache.len();
        if nodes >= 2 * self.config.k {
            // Check if we're in the edge-case of having too few different nodes.
            if self.overflow_imbalance() {
                BucketStatus::Stable
            } else {
                BucketStatus::Overflowing
            }
        } else if nodes < self.config.k {
            BucketStatus::Wanting
        } else {
            BucketStatus::Stable
        }
    }

    fn overflow_imbalance(&self) -> bool {
        let depths: Vec<usize> = self
            .active
            .iter()
            .chain(self.cache.iter())
            .map(|kn| kn.depth)
            .sorted()
            .collect();
        if let Some(first) = depths.first() {
            if let Some(first_nbr) = depths.iter().counts_by(|d| d).get(first) {
                return depths.len() - first_nbr < self.config.k;
            }
        }
        false
    }

    fn get_knode(&mut self, id: &NodeID) -> Option<&mut KNode> {
        if let Some(kn) = self.active.iter_mut().find(|kn| &kn.id == id) {
            return Some(kn);
        }
        if let Some(kn) = self.cache.iter_mut().find(|kn| &kn.id == id) {
            return Some(kn);
        }
        None
    }

    fn tick(&mut self) -> TickIDs {
        let mut delete_ids = vec![];
        for node in self.active.iter_mut() {
            node.tick();
            if node.ticks_last >= self.config.ping_timeout {
                delete_ids.push(node.id);
            }
        }
        delete_ids.iter().for_each(|id| self.remove_node(id));
        TickIDs {
            deleted: delete_ids,
            ping: self
                .active
                .iter()
                .filter_map(|kn| {
                    (kn.ticks_last > 0 && kn.ticks_last % self.config.ping_interval == 0)
                        .then(|| kn.id)
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct TickIDs {
    pub deleted: Vec<NodeID>,
    pub ping: Vec<NodeID>,
}

impl TickIDs {
    pub fn extend(&mut self, other: TickIDs) {
        self.deleted.extend_from_slice(&other.deleted);
        self.ping.extend_from_slice(&other.ping);
    }
}

impl fmt::Display for KBucket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "Active: {} :: Cache: {}",
            NodeIDs::from(self.active.iter().map(|kn| kn.id).collect::<Vec<NodeID>>()),
            NodeIDs::from(self.cache.iter().map(|kn| kn.id).collect::<Vec<NodeID>>()),
        ))?;
        Ok(())
    }
}

#[derive(PartialEq)]
enum BucketStatus {
    // #nodes > 2k
    Overflowing,
    // k <= #nodes <= 2k
    Stable,
    // #nodes < k
    Wanting,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct KNode {
    id: NodeID,
    depth: usize,
    ticks_active: u32,
    ticks_last: u32,
}

impl KNode {
    pub fn new(root: &NodeID, id: NodeID) -> Self {
        Self {
            id,
            depth: Self::get_depth(&Self::distance(root, id)),
            ticks_active: 0,
            ticks_last: 0,
        }
    }

    fn reset(&mut self) {
        self.ticks_active = 0;
        self.ticks_last = 0;
    }

    fn distance(root: &NodeID, id: NodeID) -> Vec<Bit> {
        let mut node_bytes = id.to_bytes().to_vec();
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

    fn tick(&mut self) {
        self.ticks_active += 1;
        self.ticks_last += 1;
    }

    fn active(&mut self) {
        self.ticks_last = 0;
    }
}

impl PartialEq for KNode {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[derive(Debug, PartialEq, Clone)]
enum Bit {
    Zero,
    One,
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, iter::once, str::FromStr};

    use flarch::{nodeids::U256, start_logging_filter_level};
    use rand::seq::SliceRandom;

    use super::*;

    const CONFIG: Config = Config {
        k: 2,
        ping_interval: 2,
        ping_timeout: 4,
    };

    fn rnd_node_depth(root: &NodeID, depth: usize) -> NodeID {
        loop {
            let node = NodeID::rnd();
            let dist = KNode::distance(root, node);
            let nd = KNode::get_depth(&dist);
            if nd == depth {
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
        log::debug!("{}", node);
        for i in 0..16 {
            log::debug!("{} - {}", i, rnd_node_depth(&node, i));
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
        for (depth, bucket) in kademlia.buckets.iter().enumerate() {
            for node in bucket.active.iter().chain(bucket.cache.iter()) {
                assert_eq!(
                    depth, node.depth,
                    "Node {} in bucket depth {} has wrong depth: {}",
                    node.id, depth, node.depth
                );
            }
        }
        for node in kademlia
            .root_bucket
            .active
            .iter()
            .chain(kademlia.root_bucket.cache.iter())
        {
            assert!(
                node.depth >= kademlia.buckets.len(),
                "Node {} in root has wrong depth: {} <= {}",
                node.id,
                node.depth,
                kademlia.buckets.len()
            );
        }
    }

    #[test]
    fn test_adding_nodes() {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let root = U256::from_str("00").expect("NodeID init");
        let mut kademlia = Kademlia::new(root, CONFIG);
        kademlia_add_nodes(&mut kademlia, 0, CONFIG.k);
        kademlia_test(&kademlia, 0, CONFIG.k, 0);

        kademlia_add_nodes(&mut kademlia, 2, CONFIG.k);
        kademlia_test(&kademlia, 0, CONFIG.k, CONFIG.k);

        kademlia_add_nodes(&mut kademlia, 3, 1);
        kademlia_test(&kademlia, 1, CONFIG.k, CONFIG.k - 1);

        kademlia_add_nodes(&mut kademlia, 4, CONFIG.k);
        kademlia_test(&kademlia, 3, CONFIG.k, CONFIG.k - 1);

        // TODO: make this work correctly - what is the good thing to do here?
        let mut kademlia = Kademlia::new(root, CONFIG);
        kademlia_add_nodes(&mut kademlia, 2, 3 * CONFIG.k);
        kademlia_test(&kademlia, 0, CONFIG.k, 2 * CONFIG.k);
    }

    #[test]
    fn test_removing_nodes() {
        let root = U256::from_str("00").expect("NodeID init");
        let mut kademlia = Kademlia::new(root, CONFIG);
        kademlia_add_nodes(&mut kademlia, 0, CONFIG.k);
        kademlia_add_nodes(&mut kademlia, 2, CONFIG.k);
        kademlia_add_nodes(&mut kademlia, 3, 1);
        kademlia_add_nodes(&mut kademlia, 4, CONFIG.k);

        kademlia_test(&kademlia, 3, CONFIG.k, CONFIG.k - 1);
        let id = kademlia.root_bucket.active.get(0).unwrap().id;
        kademlia.remove_node(&id);
        kademlia_test(&kademlia, 3, CONFIG.k, 0);
        let id = kademlia.root_bucket.active.get(0).unwrap().id;
        kademlia.remove_node(&id);
        kademlia_test(&kademlia, 2, CONFIG.k, CONFIG.k - 1);
    }

    // Test the standard edge-cases: no nodes, only depth==0 nodes
    #[test]
    fn test_closest_nodes_simple() {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let root = U256::from_str("00").expect("NodeID init");
        let mut kademlia = Kademlia::new(root, CONFIG);

        let nodes = kademlia.nearest_nodes(&rnd_node_depth(&root, 2));
        assert_eq!(0, nodes.len());
        kademlia_add_nodes(&mut kademlia, 0, 1);
        let nodes = kademlia.nearest_nodes(&rnd_node_depth(&root, 1));
        assert_eq!(1, nodes.len());
        kademlia_add_nodes(&mut kademlia, 2, 3 * CONFIG.k);
        let nodes = kademlia.nearest_nodes(&rnd_node_depth(&root, 1));
        assert_eq!(1, nodes.len());
    }

    // Do some more complicated cases
    #[test]
    fn test_closest_nodes_complicated() {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let root = U256::from_str("00").expect("NodeID init");
        let mut kademlia = Kademlia::new(root, CONFIG);

        kademlia_add_nodes(&mut kademlia, 2, CONFIG.k);
        kademlia_add_nodes(&mut kademlia, 3, CONFIG.k);
        // As the depth==4 nodes are entered one-by-one, the first one
        // triggers the overflow, and the second one goes to the cache.
        // So the active KBucket of the root_bucket has a depth 3 and a
        // depth 4 inside.
        kademlia_add_nodes(&mut kademlia, 4, CONFIG.k);

        let tests = vec![
            (0, 2, CONFIG.k),
            (1, 2, CONFIG.k),
            (2, 2, CONFIG.k),
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

    #[test]
    fn test_tick() {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let root = U256::from_str("00").expect("NodeID init");
        let mut kademlia = Kademlia::new(root, CONFIG);

        kademlia_add_nodes(&mut kademlia, 2, 10);
        kademlia_add_nodes(&mut kademlia, 3, 10);

        let ticks = kademlia.tick();
        assert_eq!([0, 0], [ticks.deleted.len(), ticks.ping.len()]);
        let ticks = kademlia.tick();
        assert_eq!([0, 4], [ticks.deleted.len(), ticks.ping.len()]);
        for ping in &ticks.ping[0..3] {
            kademlia.node_active(&ping);
        }
        let ticks = kademlia.tick();
        assert_eq!([0, 0], [ticks.deleted.len(), ticks.ping.len()]);
        let ticks = kademlia.tick();
        assert_eq!([1, 3], [ticks.deleted.len(), ticks.ping.len()]);
    }

    #[test]
    fn test_distribution() {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let root = NodeID::rnd();
        let mut kademlia = Kademlia::new(
            root,
            Config {
                k: 2,
                ping_interval: 10,
                ping_timeout: 20,
            },
        );

        for node in (0..256).map(|_| NodeID::rnd()) {
            kademlia.add_node(node);
        }

        for (index, bucket) in kademlia
            .buckets
            .iter()
            .chain(once(&kademlia.root_bucket))
            .enumerate()
        {
            log::info!("{index}: {}+{}", bucket.active.len(), bucket.cache.len())
        }
    }

    #[test]
    fn test_reach() {
        start_logging_filter_level(vec![], log::LevelFilter::Info);
        for k in (0..=5).rev() {
            test_reach_one(10000, 20, 2 * k + 1);
        }
    }

    fn test_reach_one(node_nbr: usize, max_hops: usize, k: usize) {
        let config = Config {
            k,
            ping_interval: 10,
            ping_timeout: 20,
        };
        let nodes: Vec<NodeID> = (0..node_nbr).map(|_| NodeID::rnd()).collect();
        let rng = &mut rand::thread_rng();
        let kademlias: Vec<Kademlia> = nodes
            .iter()
            .map(|root| {
                let mut kad = Kademlia::new(*root, config);
                let mut shuffled = nodes.clone();
                shuffled.shuffle(rng);
                kad.add_nodes(shuffled);
                kad
            })
            .collect();

            log::info!("{}", &kademlias[0]);
        let mut hops_stat = vec![];
        for (run, node) in nodes[1..node_nbr].iter().enumerate() {
            log::debug!("*** {run} ***: Searching route from {} to {node}", nodes[0]);
            let mut hop_kademlia = &kademlias[0];
            for hop in 0..max_hops {
                log::trace!("{hop_kademlia}");
                let next_hops = hop_kademlia.nearest_nodes(node);
                if let Some(next_hop) = next_hops.choose(rng) {
                    hop_kademlia = kademlias.iter().find(|k| k.root == *next_hop).unwrap();
                    log::trace!("{hop}: {next_hop}");
                } else {
                    assert_eq!(node, &hop_kademlia.root);
                    log::trace!("{hop}: found");
                    hops_stat.push(hop);
                    break;
                }
                assert_ne!(
                    max_hops - 1,
                    hop,
                    "Didn't find route between {} and {node}",
                    nodes[0]
                );
            }
        }
        log::info!(
            "Stats for {}: {:?}",
            config.k,
            hops_stat.iter().counts_by(|u| u).iter().sorted()
        );
    }
}
