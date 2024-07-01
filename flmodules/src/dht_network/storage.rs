use std::fmt::Debug;

use crate::nodeids::NodeID;

pub struct Kademlia {
    // All buckets starting with 0..depth-1 0s
    buckets: Vec<KBucket>,
    // The home of the root bucket, including the optimization if the root
    // bucket is isolated
    root_bucket: RootBucket,
    // The root node of this Kademlia instance
    root: Box<dyn ID>,
    root_bytes: Vec<u8>,
    // Maximum number of nodes per bucket
    k: usize,
}

impl Kademlia {
    pub fn new(k: usize, root: Box<dyn ID>) -> Self {
        Self {
            buckets: vec![],
            root_bucket: RootBucket::new(k, 0),
            root_bytes: root.get_bytes(),
            root,
            k,
        }
    }

    pub fn add_node(&mut self, id: Box<dyn ID>) {
        let node = KNode::new(&self.root_bytes, id);
        let state = if node.depth < self.buckets.len() {
            self.buckets.get_mut(node.depth).unwrap().add_node(node)
        } else {
            self.root_bucket.add_node(node)
        };
        match state {
            BucketStatus::Stable => todo!(),
            BucketStatus::Empty => todo!(),
            BucketStatus::Overflowing => todo!(),
        }
    }

    pub fn remove_node(&mut self, id: Box<dyn ID>) {
        let node = KNode::new(&self.root_bytes, id);
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

    pub fn nearest_nodes(&mut self, _node: Box<dyn ID>) -> Vec<Box<dyn ID>> {
        vec![]
    }

    pub fn ping_answer(&mut self, _node: Box<dyn ID>) {}

    pub fn tick(&mut self) -> Vec<Box<dyn ID>> {
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
        } else {
            self.cache.push(node);
        }
        BucketStatus::Stable
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

        if self.active.len() + self.cache.len() > 0 {
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
                    p = match node.distance.get(d).unwrap(){
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
    node: Box<dyn ID>,
    depth: usize,
    distance: Vec<Bit>,
    state: NodeState,
}

impl KNode {
    pub fn new(root: &Vec<u8>, node: Box<dyn ID>) -> Self {
        let distance = Self::distance(root, node.get_bytes());
        let mut depth = 0;
        while let Some(bit) = distance.get(depth) {
            if bit == &Bit::Zero {
                depth += 1;
            } else {
                break;
            }
        }
        Self {
            node,
            state: NodeState::Active,
            depth,
            distance,
        }
    }

    fn distance(root: &Vec<u8>, mut node: Vec<u8>) -> Vec<Bit> {
        root.iter()
            .map(|c| (c ^ node.remove(0)).count_ones())
            .flat_map(|mut x| {
                (0..7).map(move |_| {
                    x *= 2;
                    if x & 0x100 > 0 {
                        Bit::Zero
                    } else {
                        Bit::One
                    }
                })
            })
            .collect()
    }

    fn set_state(&mut self, state: NodeState) {
        self.state = state;
    }
}

impl PartialEq for KNode {
    fn eq(&self, other: &Self) -> bool {
        self.node.equals(&*other.node)
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

pub trait ID: Debug {
    fn get_bytes(&self) -> Vec<u8>;
    fn equals(&self, other: &dyn ID) -> bool {
        let mut other_bytes = other.get_bytes().to_vec();
        self.get_bytes()
            .to_vec()
            .iter()
            .all(|b| b == &other_bytes.remove(0))
    }
}

impl ID for NodeID {
    fn get_bytes(&self) -> Vec<u8> {
        self.to_bytes().to_vec()
    }
}
