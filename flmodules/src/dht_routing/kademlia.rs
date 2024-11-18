use std::fmt::Debug;

use flarch::nodeids::NodeID;

pub struct Kademlia {
    // All buckets starting with a distance of 0..depth-1 0s followed by at least
    // one 1.
    buckets: Vec<KBucket>,
    // The home of the root bucket, including the optimization if the root
    // bucket is isolated
    root_bucket: RootBucket,
    // The root node of this Kademlia instance
    root: NodeID,
    // Maximum number of nodes per bucket
    k: usize,
}

impl Kademlia {
    pub fn new(k: usize, root: NodeID) -> Self {
        Self {
            buckets: vec![],
            root_bucket: RootBucket::new(k, 0),
            root,
            k,
        }
    }

    pub fn add_node(&mut self, id: NodeID) {
        let node = KNode::new(&self.root, id);
        let bucket = if node.depth < self.buckets.len() {
            self.buckets.get_mut(node.depth).unwrap()
        } else {
            &mut self.root_bucket.root
        };
        match bucket.add_node(node) {
            BucketStatus::Stable => {},
            BucketStatus::Empty => panic!("Shouldn't get empty bucket when adding a node"),
            BucketStatus::Overflowing => {
            },
        }
    }

    pub fn remove_node(&mut self, id: NodeID) {
        let node = KNode::new(&self.root, id);
        let state = if node.depth < self.buckets.len() {
            self.buckets.get_mut(node.depth).unwrap().remove_node(node)
        } else {
            self.root_bucket.remove_node(node)
        };
        match state {
            BucketStatus::Stable => todo!(),
            BucketStatus::Empty => todo!(),
            BucketStatus::Overflowing => todo!(),
        }
    }

    pub fn nearest_nodes(&mut self, _node: NodeID) -> Vec<NodeID> {
        vec![]
    }

    pub fn ping_answer(&mut self, _node: NodeID) {}

    pub fn tick(&mut self) -> Vec<NodeID> {
        vec![]
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

    fn add_node(&mut self, node: KNode) -> BucketStatus {
        if self.active.len() < self.k {
            self.active.push(node);
            BucketStatus::Stable
        } else {
            self.cache.push(node);
            BucketStatus::Overflowing
        }
    }

    fn remove_node(&mut self, node: KNode) -> BucketStatus {
        if let Some(pos) = self.active.iter().position(|n| n == &node) {
            self.active.remove(pos);
            if self.cache.len() > 0 {
                self.active.push(self.cache.pop().unwrap());
            }
        } else if let Some(pos) = self.cache.iter().position(|n| n == &node) {
            self.cache.remove(pos);
        }

        if self.active.len() + self.cache.len() > self.k {
            BucketStatus::Overflowing
        } else if self.active.len() > 0 {
            BucketStatus::Stable
        } else {
            BucketStatus::Empty
        }
    }
}

struct RootBucket {
    tree: KTree,
    root: KBucket,
    k: usize,
}

impl RootBucket {
    fn new(k: usize, depth: usize) -> Self {
        Self {
            tree: KTree::new(k, depth),
            root: KBucket::new(k),
            k,
        }
    }

    fn add_node(&mut self, node: KNode) -> BucketStatus {
        if node.distance.get(self.k).unwrap() == &Bit::One {
            self.tree.add_node(node)
        } else {
            self.root.add_node(node)
        }
    }

    fn remove_node(&mut self, node: KNode) -> BucketStatus {
        if node.distance.get(self.k).unwrap() == &Bit::One {
            self.tree.remove_node(node)
        } else {
            self.root.remove_node(node)
        }
    }
}

struct KTree {
    top: KTreeNode,
    k: usize,
    depth: usize,
}

impl KTree {
    fn new(k: usize, depth: usize) -> Self {
        Self {
            top: KTreeNode::Leaf(vec![]),
            k,
            depth,
        }
    }

    fn add_node(&mut self, node: KNode) -> BucketStatus {
        let mut d = self.depth;
        let mut p = &mut self.top;
        loop {
            match p {
                KTreeNode::Leaf(l) => {
                    if l.len() < self.k {
                        l.push(node);
                        break;
                    }
                    *p = KTreeNode::split_bucket(l.drain(..).collect(), d);
                }
                KTreeNode::Node(one, zero) => {
                    p = match node.distance.get(d).unwrap() {
                        Bit::Zero => zero,
                        Bit::One => one,
                    };
                }
            }
            match node.distance.get(d).unwrap() {
                Bit::One => {}
                Bit::Zero => {}
            }
            d += 1;
        }
        BucketStatus::Stable
    }

    fn remove_node(&mut self, _node: KNode) -> BucketStatus {
        BucketStatus::Stable
    }
}

enum KTreeNode {
    Leaf(Vec<KNode>),
    Node(Box<KTreeNode>, Box<KTreeNode>),
}

impl KTreeNode {
    // TODO: do something useful here with the bucket:
    // - separate it into 1s and 0s bucket
    // - make sure that if it's all in one bucket, it continues recursively
    fn split_bucket(bucket: Vec<KNode>, _depth: usize) -> KTreeNode {
        KTreeNode::Node(
            Box::new(KTreeNode::Leaf(bucket)),
            Box::new(KTreeNode::Leaf(vec![])),
        )
    }
}

enum BucketStatus {
    Stable,
    Empty,
    Overflowing,
}

#[derive(Debug)]
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

#[derive(Debug, PartialEq)]
enum Bit {
    Zero,
    One,
}

#[derive(Debug)]
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

    fn distance_str(root: &NodeID, node_str: &str) -> usize {
        let node = U256::from_str(node_str).expect("NodeID init");
        KNode::get_depth(&KNode::distance(root, node))
    }

    #[test]
    fn test_distance() {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let root = U256::from_str("00").expect("NodeID init");
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
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let node = U256::from_str("00").expect("NodeID init");
        log::info!("{}", node);
        for i in 0..16 {
            log::info!("{} - {}", i, rnd_node_depth(&node, i));
        }
    }
}
