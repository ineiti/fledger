use core::cmp::min;
use flutils::nodeids::{NodeID, NodeIDs};

#[derive(PartialEq, Clone, Debug)]
pub struct NodeTime {
    pub id: NodeID,
    pub ticks: u32,
}

#[derive(PartialEq, Clone, Debug)]
pub struct Nodes(pub Vec<NodeTime>);

impl Nodes {
    pub fn new() -> Nodes {
        Self { 0: vec![] }
    }

    pub fn get_nodes(&self) -> NodeIDs {
        NodeIDs {
            0: self.0.iter().map(|n| n.id).collect(),
        }
    }

    pub fn remove_missing(&mut self, nodes: &NodeIDs) -> NodeIDs {
        let removed = self.get_nodes().remove_missing(nodes);
        self.0.retain(|nt| !removed.0.contains(&nt.id));
        removed
    }

    pub fn add_new(&mut self, nodes: Vec<NodeID>) {
        let mut nts = nodes
            .iter()
            .map(|n| NodeTime { id: *n, ticks: 0 })
            .collect();
        self.0.append(&mut nts);
    }

    pub fn contains(&self, node: &NodeID) -> bool {
        self.get_nodes().0.contains(node)
    }

    // Removes the oldest n nodes. If less than n nodes are stored, only remove
    // these nodes.
    // It returns the removed nodes.
    pub fn remove_oldest_n(&mut self, mut n: usize) -> NodeIDs {
        self.0.sort_by(|a, b| b.ticks.cmp(&a.ticks));
        n = min(n, self.0.len());
        NodeIDs {
            0: self.0.splice(..n, vec![]).map(|n| n.id).collect(),
        }
    }

    // Only keeps the newest n nodes. If there are more than n nodes, the
    // oldest nodes are discarded.
    pub fn keep_newest_n(&mut self, n: usize) -> NodeIDs {
        if self.0.len() > n {
            self.remove_oldest_n(n - self.0.len())
        } else {
            NodeIDs::empty()
        }
    }

    // Returns nodes that are as old or older than age.
    pub fn oldest_ticks(&mut self, ticks: u32) -> NodeIDs {
        self.0.sort_by(|a, b| b.ticks.cmp(&a.ticks));
        let nodes: Vec<NodeID> = self
            .0
            .iter()
            .take_while(|n| n.ticks >= ticks)
            .map(|n| n.id)
            .collect();
        NodeIDs { 0: nodes }
    }

    // Removes nodes that are as old or older than age and returns the removed nodes.
    pub fn remove_oldest_ticks(&mut self, ticks: u32) -> NodeIDs {
        let nodes = self.oldest_ticks(ticks);
        self.0.splice(..nodes.0.len(), vec![]);
        nodes
    }

    // Increases the tick of all nodes by 1
    pub fn tick(&mut self) {
        for node in self.0.iter_mut() {
            node.ticks += 1;
        }
    }
}
