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
            k: 2,
            ping_interval: 10,
            ping_timeout: 30,
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

    #[cfg(feature = "testing")]
    pub fn add_nodes_active(&mut self, ids: Vec<NodeID>) {
        for id in ids {
            let node = KNode::new(&self.root, id);
            if let Some(bucket) = self.buckets.get_mut(node.depth) {
                bucket.add_node_active(node);
            } else {
                self.root_bucket.add_node_active(node);
                if self.root_bucket.status() == BucketStatus::Overflowing {
                    self.rebalance_overflow();
                }
            }
        }
    }

    pub fn node_disconnected(&mut self, id: NodeID) {
        let node = KNode::new(&self.root, id);
        if let Some(bucket) = self.buckets.get_mut(node.depth) {
            bucket.remove_node(&node.id);
            if self.root_bucket.status() == BucketStatus::Wanting {
                self.rebalance_wanting();
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

    /// Returns the next hop to get the message as close to the destination as possible.
    /// If a needed bucket is empty, the closest nodes on the 'wrong' branch will be returned.
    pub fn route_closest(&self, dst: &NodeID, last: Option<&NodeID>) -> Vec<NodeID> {
        let depth = KNode::get_depth(&self.root, *dst);
        match last {
            Some(l) => {
                let depth_last = depth.max(KNode::get_depth(&self.root, *l));
                let depth_root_last = self.root_bucket.last_depth().unwrap_or(depth_last);
                for d in std::iter::once(depth).chain(depth_last + 1..depth_root_last) {
                    let ret = self.get_nodes_depth(d);
                    if ret.len() > 0 {
                        return ret;
                    }
                }
                vec![]
            }
            None => self.get_nodes_depth(depth),
        }
    }

    /// Returns the next hop to get the message to the exact destination as possible.
    /// If a needed bucket is empty, an empty vector is returned, and the routing will fail.
    pub fn route_direct(&self, dst: &NodeID) -> Vec<NodeID> {
        let depth = KNode::get_depth(&self.root, *dst);
        self.get_nodes_depth(depth)
    }

    /// Returns all nodes at a given depth.
    pub fn get_nodes_depth(&self, depth: usize) -> Vec<NodeID> {
        if let Some(ret) = self.buckets.get(depth).map(|b| b.get_active_ids(depth)) {
            ret
        } else {
            self.root_bucket.get_active_ids(depth)
        }
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

    /// Returns all the active nodes of the kademlia system.
    pub fn bucket_nodes(&self) -> Vec<Vec<NodeID>> {
        self.buckets
            .iter()
            .map(|b| &b.active)
            .chain(vec![&self.root_bucket.active])
            .map(|kns| kns.iter().map(|kn| kn.id).collect::<Vec<_>>())
            .collect()
    }

    /// Returns all the nodes waiting to be activated of the kademlia system.
    pub fn cache_nodes(&self) -> Vec<NodeID> {
        self.buckets
            .iter()
            .flat_map(|b| &b.cache)
            .chain(&self.root_bucket.cache)
            .map(|kn| kn.id)
            .collect()
    }

    /// The system received a message from this node, so its considered
    /// active.
    pub fn node_active(&mut self, id: &NodeID) -> bool {
        let depth = KNode::get_depth(&self.root, *id);
        self.buckets
            .get_mut(depth)
            .unwrap_or_else(|| &mut self.root_bucket)
            .node_active(id)
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

    // The root bucket overflows - split it until it has between k and 2k
    // nodes.
    fn rebalance_overflow(&mut self) {
        while self.root_bucket.status() == BucketStatus::Overflowing {
            let mut bucket = KBucket::new(self.config);
            let (active, cache) = self
                .root_bucket
                .remove_all_nodes_at_depth(self.buckets.len());
            bucket.add_nodes_active(active);
            bucket.add_nodes(cache);
            self.buckets.push(bucket);
        }
    }

    // The root bucket is wanting - merge with non-empty upper buckets until
    // it's safe again.
    fn rebalance_wanting(&mut self) {
        while self.root_bucket.status() == BucketStatus::Wanting {
            if let Some(bucket) = self.buckets.pop() {
                self.root_bucket.add_nodes_active(bucket.active);
                self.root_bucket.add_nodes(bucket.cache);
            } else {
                return;
            }
        }
    }

    fn _get_active_bucket_ids(&self, depth: usize) -> Option<Vec<NodeID>> {
        if let Some(bucket) = self.buckets.get(depth) {
            if bucket.active.len() > 0 {
                return Some(bucket.active.iter().map(|kn| kn.id).collect());
            }
        }
        None
    }

    fn has_node_id(&self, id: &NodeID) -> bool {
        for bucket in &self.buckets {
            if bucket.has_node_id(id) {
                return true;
            }
        }
        self.root_bucket.has_node_id(id)
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
        if self.is_root || self.cache.len() < 2 * self.config.k {
            self.cache.push(node);
        }
    }

    fn add_nodes(&mut self, nodes: Vec<KNode>) {
        for node in nodes.into_iter() {
            self.add_node(node);
        }
    }

    fn add_nodes_active(&mut self, nodes: Vec<KNode>) {
        for node in nodes {
            self.add_node_active(node);
        }
    }

    fn add_node_active(&mut self, node: KNode) {
        if self.is_root || self.active.len() < self.config.k {
            self.active.push(node);
            self.active.sort_by(|a, b| a.depth.cmp(&b.depth));
        } else if self.cache.len() < self.config.k {
            self.cache.push(node);
        }
    }

    fn remove_node(&mut self, id: &NodeID) {
        self.active.retain(|kn| &kn.id != id);
        self.cache.retain(|kn| &kn.id != id);
    }

    // Removes the node at depth, but avoids draining the
    // node lists below config.k nodes.
    // It may still happen that all active nodes get drained, and only
    // cached nodes remain.
    fn remove_all_nodes_at_depth(&mut self, depth: usize) -> (Vec<KNode>, Vec<KNode>) {
        let max_nodes = (self.cache.len() + self.active.len()).saturating_sub(self.config.k);
        let mut cache = vec![];
        for _ in 0..max_nodes {
            if let Some(pos) = self.cache.iter().position(|kn| kn.depth == depth) {
                cache.push(self.cache.remove(pos));
            }
        }

        let mut active = vec![];
        for _ in 0..max_nodes - cache.len() {
            if let Some(pos) = self.active.iter().position(|kn| kn.depth == depth) {
                active.push(self.active.remove(pos));
            }
        }

        (active, cache)
    }

    fn get_active_ids(&self, depth: usize) -> Vec<NodeID> {
        self.active
            .iter()
            .filter_map(|kn| (kn.depth == depth).then(|| kn.id))
            .collect()
    }

    fn last_depth(&self) -> Option<usize> {
        self.active.last().map(|kn| kn.depth)
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

    fn _get_knode(&mut self, id: &NodeID) -> Option<&mut KNode> {
        self.active
            .iter_mut()
            .chain(self.cache.iter_mut())
            .find(|kn| &kn.id == id)
    }

    fn tick(&mut self) -> TickIDs {
        let mut ticks = self.tick_active();
        ticks.extend(self.tick_cache());
        for id in &ticks.deleted {
            self.remove_node(id);
        }
        ticks
    }

    fn tick_active(&mut self) -> TickIDs {
        let mut tick_ids = TickIDs::default();

        // Active nodes get only pinged when they were active for ping_interval.
        for node in self.active.iter_mut() {
            node.tick();
            if node.ticks_last % self.config.ping_interval == 0 {
                tick_ids.ping.push(node.id);
            }
            if node.ticks_last >= self.config.ping_timeout {
                tick_ids.deleted.push(node.id);
            }
        }

        tick_ids
    }

    fn tick_cache(&mut self) -> TickIDs {
        let mut tick_ids = TickIDs::default();

        // Cached nodes get pinged directly.
        // Up to twice the missig nodes get pinged to fill up self.active.
        let fillup_nodes = if self.is_root {
            self.cache.len()
        } else {
            2 * (self.config.k - self.active.len())
        };
        for node in self.cache.iter_mut().take(fillup_nodes) {
            if node.ticks_last % self.config.ping_interval == 0 {
                tick_ids.ping.push(node.id);
            }
            node.tick();
            if node.ticks_last >= self.config.ping_timeout {
                tick_ids.deleted.push(node.id);
            }
        }

        tick_ids
    }

    fn node_active(&mut self, id: &NodeID) -> bool {
        if let Some(kn) = self.active.iter_mut().find(|kn| &kn.id == id) {
            kn.active();
        } else if let Some(pos) = self.cache.iter().position(|kn| &kn.id == id) {
            let mut kn = self.cache.remove(pos);
            kn.active();
            if self.is_root || self.active.len() < self.config.k {
                self.active.push(kn);
                self.active.sort_by(|a, b| a.depth.cmp(&b.depth));
                return true;
            } else {
                self.cache.push(kn);
            }
        }
        false
    }

    fn has_node_id(&self, id: &NodeID) -> bool {
        self.active
            .iter()
            .chain(self.cache.iter())
            .any(|kn| &kn.id == id)
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
pub struct KNode {
    id: NodeID,
    depth: usize,
    ticks_active: u32,
    ticks_last: u32,
}

impl KNode {
    pub fn new(root: &NodeID, id: NodeID) -> Self {
        Self {
            id,
            depth: Self::get_depth(root, id),
            ticks_active: 0,
            ticks_last: 0,
        }
    }

    fn _reset(&mut self) {
        self.ticks_active = 0;
        self.ticks_last = 0;
    }

    pub fn get_depth(root: &NodeID, id: NodeID) -> usize {
        for (index, (root_byte, id_byte)) in
            root.as_ref().iter().zip(id.as_ref().iter()).enumerate()
        {
            let xor_result = root_byte ^ id_byte;

            // Instead of checking each bit individually, iterate over the bits.
            for i in (0..8).rev() {
                if (xor_result & (1 << i)) != 0 {
                    return index * 8 + 7 - i;
                }
            }
        }
        return id.as_ref().len() * 8 - 1;
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

#[cfg(test)]
mod test {
    use std::{iter::once, str::FromStr};

    use flarch::{nodeids::U256, start_logging_filter_level};
    use rand::seq::SliceRandom;

    use super::*;

    const LOG_LVL: log::LevelFilter = log::LevelFilter::Info;

    const CONFIG: Config = Config {
        k: 2,
        ping_interval: 2,
        ping_timeout: 4,
    };

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

    #[test]
    fn test_get_depth() {
        start_logging_filter_level(vec![], LOG_LVL);

        let root = U256::from_str("00").expect("NodeID init");
        assert_eq!(255, KNode::get_depth(&root, root));
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

    #[test]
    fn test_rnd_node_depth() {
        start_logging_filter_level(vec![], LOG_LVL);

        let node = U256::from_str("00").expect("NodeID init");
        log::debug!("{}", node);
        for i in 0..16 {
            log::debug!("{} - {}", i, rnd_node_depth(&node, i));
        }
    }

    fn kademlia_test(kademlia: &Kademlia, buckets: usize, root_active: usize) {
        kademlia_test_cache(kademlia, buckets, root_active, 0);
    }
    fn kademlia_test_cache(
        kademlia: &Kademlia,
        buckets: usize,
        root_active: usize,
        root_cache: usize,
    ) {
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
        start_logging_filter_level(vec![], LOG_LVL);

        let root = U256::from_str("00").expect("NodeID init");
        let mut kademlia = Kademlia::new(root, CONFIG);
        kademlia_add_nodes(&mut kademlia, 0, CONFIG.k);
        kademlia_test(&kademlia, 0, CONFIG.k);

        kademlia_add_nodes(&mut kademlia, 2, CONFIG.k);
        kademlia_test(&kademlia, 1, CONFIG.k);

        kademlia_add_nodes(&mut kademlia, 3, 1);
        kademlia_test(&kademlia, 1, CONFIG.k + 1);

        kademlia_add_nodes(&mut kademlia, 4, CONFIG.k);
        kademlia_test(&kademlia, 3, CONFIG.k + 1);

        let mut kademlia = Kademlia::new(root, CONFIG);
        kademlia_add_nodes(&mut kademlia, 2, 3 * CONFIG.k);
        kademlia_test(&kademlia, 0, 3 * CONFIG.k);
    }

    #[test]
    fn test_removing_nodes() {
        let root = U256::from_str("00").expect("NodeID init");
        let mut kademlia = Kademlia::new(root, CONFIG);
        kademlia_add_nodes(&mut kademlia, 0, CONFIG.k);
        kademlia_add_nodes(&mut kademlia, 2, CONFIG.k);
        kademlia_add_nodes(&mut kademlia, 3, 1);
        kademlia_add_nodes(&mut kademlia, 4, CONFIG.k);

        kademlia_test(&kademlia, 3, CONFIG.k + 1);
        let id = kademlia.root_bucket.active.get(0).unwrap().id;
        kademlia.remove_node(&id);
        kademlia_test(&kademlia, 3, CONFIG.k);
        let id = kademlia.root_bucket.active.get(0).unwrap().id;
        kademlia.remove_node(&id);
        kademlia_test(&kademlia, 2, CONFIG.k + 1);
    }

    // Test the standard edge-cases: no nodes, only depth==0 nodes
    #[test]
    fn test_closest_nodes_simple() {
        start_logging_filter_level(vec![], LOG_LVL);

        let root = U256::from_str("00").expect("NodeID init");
        let mut kademlia = Kademlia::new(root, CONFIG);

        let dst_0 = rnd_node_depth(&root, 0);
        let dst_1 = rnd_node_depth(&root, 1);
        let dst_2 = rnd_node_depth(&root, 2);

        let nodes = kademlia.route_closest(&dst_2, None);
        assert_eq!(0, nodes.len());

        kademlia_add_nodes(&mut kademlia, 0, 1);
        let nodes = kademlia.route_closest(&dst_0, None);
        assert_eq!(1, nodes.len());
        let nodes = kademlia.route_closest(&dst_1, None);
        assert_eq!(0, nodes.len());

        kademlia_add_nodes(&mut kademlia, 2, 1);
        let nodes = kademlia.route_closest(&dst_2, None);
        assert_eq!(1, nodes.len());

        kademlia_add_nodes(&mut kademlia, 2, 2 * CONFIG.k);
        let nodes = kademlia.route_closest(&dst_2, None);
        assert_eq!(2 * CONFIG.k + 1, nodes.len());
    }

    // Do some more complicated cases
    #[test]
    fn test_closest_nodes_last() {
        start_logging_filter_level(vec![], LOG_LVL);

        let root = U256::from_str("00").expect("NodeID init");
        let mut kademlia = Kademlia::new(root, CONFIG);

        kademlia_add_nodes(&mut kademlia, 2, CONFIG.k);
        kademlia_add_nodes(&mut kademlia, 3, CONFIG.k);
        // As the depth==4 nodes are entered one-by-one, the first one
        // triggers the overflow, and the second one goes to the cache.
        // So the active KBucket of the root_bucket has a depth 3 and a
        // depth 4 inside.
        kademlia_add_nodes(&mut kademlia, 4, CONFIG.k + 1);

        let tests = vec![
            (0, 2, CONFIG.k),
            (1, 2, CONFIG.k),
            (2, 2, CONFIG.k),
            (3, 3, CONFIG.k),
            (4, 4, CONFIG.k + 1),
            (5, 4, 0),
        ];

        for (depth, returned, nbr) in tests {
            let dst = rnd_node_depth(&root, depth);
            let nodes = kademlia.route_closest(&dst, Some(&dst));
            assert_eq!(
                nbr,
                nodes.len(),
                "{depth} - {returned} - {nbr} - {}\n{kademlia}",
                NodeIDs::from(nodes),
            );
            for node in nodes {
                let d = KNode::get_depth(&root, node);
                assert_eq!(
                    returned, d,
                    "Node {node} with depth {d} in {depth} - {returned} - {nbr}"
                );
            }
        }
    }

    fn tick_len(kademlia: &mut Kademlia, ping: usize, deleted: usize) -> TickIDs {
        let ticks = kademlia.tick();
        assert_eq!(
            ping,
            ticks.ping.len(),
            "Got ping for {} instead of {ping} nodes",
            ticks.ping.len()
        );
        assert_eq!(
            deleted,
            ticks.deleted.len(),
            "Got deleted for {} instead of {deleted} nodes",
            ticks.deleted.len()
        );
        ticks
    }

    #[test]
    fn test_tick() {
        start_logging_filter_level(vec![], LOG_LVL);

        let root = U256::from_str("00").expect("NodeID init");
        let mut kademlia = Kademlia::new(root, CONFIG);

        kademlia_add_nodes(&mut kademlia, 2, 10);
        kademlia_add_nodes(&mut kademlia, 3, 10);

        tick_len(&mut kademlia, 0, 0);
        let ticks = tick_len(&mut kademlia, 12, 0);
        for ping in &ticks.ping[0..3] {
            kademlia.node_active(&ping);
        }
        tick_len(&mut kademlia, 0, 0);
        tick_len(&mut kademlia, 12, 9);
    }

    #[test]
    fn test_distribution() {
        start_logging_filter_level(vec![], LOG_LVL);

        let root = NodeID::rnd();
        let mut kademlia = Kademlia::new(
            root,
            Config {
                k: 2,
                ping_interval: 10,
                ping_timeout: 20,
            },
        );

        kademlia.add_nodes_active((0..256).map(|_| NodeID::rnd()).collect());

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
        // use rayon::prelude::*;

        let mut res = vec![];
        let total = 100;
        (0..5).rev().for_each(|ki| {
            let k = (total / 50) * ki + 1;
            (0..=5).collect::<Vec<_>>().iter().for_each(|&missing| {
                log::info!("{ki}/{missing}");
                let visible = total - missing * 50 / (1000 / total);
                let missed = reach_one(total, visible, 200, 2);
                res.push([k, visible, missed]);
            });
        });
        log::info!("\n{res:?}");
    }

    fn reach_one(node_nbr: usize, node_visible: usize, max_hops: usize, k: usize) -> usize {
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
                shuffled.retain(|id| id != root);
                kad.add_nodes_active(shuffled.partial_shuffle(rng, node_visible).0.to_vec());
                // kad.add_nodes(shuffled);
                kad
            })
            .collect();

        // log::debug!("{}", &kademlias[0]);
        let mut hops_stat = vec![];
        let mut missing = 0;
        for (run, node) in nodes[1..node_nbr].iter().enumerate() {
            log::debug!("*** {run} ***: Searching route from {} to {node}", nodes[0]);
            let mut hop_kademlia = kademlias.choose(rng).unwrap();
            for hop in 0..max_hops {
                // log::trace!("{hop_kademlia}");
                let next_hops =
                    hop_kademlia.route_closest(node, (hop > 0).then(|| &hop_kademlia.root));
                if let Some(next_hop) = next_hops.choose(rng) {
                    hop_kademlia = kademlias.iter().find(|k| k.root == *next_hop).unwrap();
                    log::trace!("{hop}/{}: {next_hop}", KNode::get_depth(node, *next_hop));
                } else {
                    if node != &hop_kademlia.root {
                        log::trace!(
                            "{hop}: empty node returned before end: {} != {}",
                            node,
                            hop_kademlia.root
                        );
                        missing += 1;
                        // log::info!("{}", hop_kademlia);
                        // panic!();
                        break;
                    }
                    log::trace!("{hop}: found");
                    hops_stat.push(hop);
                    break;
                }
                if max_hops - 1 == hop {
                    log::trace!(
                        "Run {run}: Didn't find destination for this node in {max_hops} hops"
                    );
                    missing += 1;
                    // panic!();
                }
            }
        }
        log::debug!(
            "Stats for {}: missing, route-length\n{missing} - {:?}",
            config.k,
            hops_stat.iter().counts_by(|u| u).iter().sorted()
        );

        missing
    }

    fn kademlia_bucket(kademlia: &Kademlia, depth: usize, active: usize, cache: usize) {
        let bucket = kademlia
            .buckets
            .get(depth)
            .expect(&format!("Bucket with depth {depth} doesn't exist"));
        assert_eq!(
            active,
            bucket.active.len(),
            "{} active instead of {active}",
            bucket.active.len()
        );
        assert_eq!(
            cache,
            bucket.cache.len(),
            "{} cache instead of {cache}",
            bucket.cache.len()
        );
    }

    #[test]
    fn test_add_node_activate() {
        start_logging_filter_level(vec![], LOG_LVL);

        let root = NodeID::rnd();
        let mut kademlia = Kademlia::new(root, CONFIG);

        // Create some nodes
        kademlia_add_nodes_cache(&mut kademlia, 2, 2 * CONFIG.k);
        kademlia_add_nodes_cache(&mut kademlia, 3, 2 * CONFIG.k);
        kademlia_test_cache(&kademlia, 3, 0, 2 * CONFIG.k);
        let ticks = tick_len(&mut kademlia, 4 * CONFIG.k, 0);

        // Ping some of the nodes
        for id in &ticks.ping[0..2] {
            kademlia.node_active(id);
        }
        for id in &ticks.ping[6..8] {
            kademlia.node_active(id);
        }
        kademlia_test_cache(&kademlia, 3, CONFIG.k, CONFIG.k);
        kademlia_bucket(&kademlia, 2, CONFIG.k, CONFIG.k);

        // Ping rest of nodes
        for id in &ticks.ping[2..6] {
            kademlia.node_active(id);
        }
        kademlia_test_cache(&kademlia, 3, 2 * CONFIG.k, 0);
        kademlia_bucket(&kademlia, 2, CONFIG.k, CONFIG.k);

        tick_len(&mut kademlia, 0, 0);
        log::info!("{kademlia}");
        tick_len(&mut kademlia, 3 * CONFIG.k, 0);
    }
}
