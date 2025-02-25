use core::cmp::min;
use flarch::nodeids::{NodeID, NodeIDs};
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct NodeTime {
    pub id: NodeID,
    pub ticks: u32,
}

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
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

    pub fn remove(&mut self, nodes: &NodeIDs) {
        self.0.retain(|nt| !nodes.0.contains(&nt.id));
    }

    pub fn add_new(&mut self, nodes: Vec<NodeID>) {
        let nodes_new: Vec<NodeID> = nodes.into_iter().filter(|n| !self.contains(n)).collect();
        let mut nts = nodes_new
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

    // Removes the newest n nodes. If less than n nodes are stored, only remove
    // these nodes.
    // It returns the removed nodes.
    pub fn remove_newest_n(&mut self, mut n: usize) -> NodeIDs {
        self.0.sort_by(|a, b| a.ticks.cmp(&b.ticks));
        n = min(n, self.0.len());
        NodeIDs {
            0: self.0.splice(..n, vec![]).map(|n| n.id).collect(),
        }
    }

    // Only keeps the newest n nodes. If there are more than n nodes, the
    // oldest nodes are discarded.
    pub fn keep_newest_n(&mut self, n: usize) -> NodeIDs {
        if self.0.len() > n {
            self.remove_oldest_n(self.0.len() - n)
        } else {
            NodeIDs::empty()
        }
    }

    // Only keeps the oldest n nodes. If there are more than n nodes, the
    // newest nodes are discarded.
    pub fn keep_oldest_n(&mut self, n: usize) -> NodeIDs {
        if self.0.len() > n {
            self.remove_newest_n(self.0.len() - n)
        } else {
            NodeIDs::empty()
        }
    }

    // Returns nodes that are as old or older than age.
    pub fn oldest_ticks(&self, ticks: u32) -> NodeIDs {
        let nodes: Vec<NodeID> = self
            .sorted(false)
            .0
            .iter()
            .take_while(|n| n.ticks >= ticks)
            .map(|n| n.id)
            .collect();
        NodeIDs { 0: nodes }
    }

    pub fn sorted(&self, rising: bool) -> Nodes {
        let mut sorted = self.0.clone();
        sorted.sort_by(|a, b| a.ticks.cmp(&b.ticks));
        if !rising {
            sorted.reverse();
        }
        Nodes { 0: sorted }
    }

    // Counts number of nodes that are as old or older than ticks.
    pub fn count_expired(&self, ticks: u32) -> usize {
        self.0
            .iter()
            .fold(0, |a, n| (n.ticks >= ticks).then(|| a + 1).unwrap_or(a))
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

#[cfg(test)]
mod tests {
    use flarch::start_logging;

    use super::*;

    #[test]
    fn expired() {
        let mut n = Nodes::new();
        n.add_new(NodeIDs::new(3).0);
        let mut ticks = 1;
        for node in n.0.iter_mut() {
            node.ticks = ticks;
            ticks += 1;
        }

        assert_eq!(0, n.count_expired(4));
        assert_eq!(1, n.count_expired(3));
        assert_eq!(2, n.count_expired(2));
        assert_eq!(3, n.count_expired(1));
    }

    fn make_nodes(n: usize) -> Nodes {
        let mut nodes = Nodes::new();
        nodes.add_new(NodeIDs::new(n as u32).0);
        for node in 0..n {
            nodes.0[node].ticks = node as u32 + 1;
        }
        nodes
    }

    // Tests the nodes and the remove methods
    #[test]
    fn test_nodes_remove() -> anyhow::Result<()> {
        start_logging();

        let mut nodes = make_nodes(5);
        let mut removed = nodes.remove_oldest_n(2);
        assert_eq!(removed.0.len(), 2);
        assert_eq!(nodes.0.len(), 3);

        removed = nodes.remove_oldest_n(5);
        assert_eq!(removed.0.len(), 3);
        assert_eq!(nodes.0.len(), 0);

        nodes = make_nodes(5);
        removed = nodes.remove_oldest_ticks(6);
        assert_eq!(nodes.0.len(), 5);
        assert_eq!(removed.0.len(), 0);

        removed = nodes.remove_oldest_ticks(4);
        assert_eq!(nodes.0.len(), 3);
        assert_eq!(removed.0.len(), 2);

        Ok(())
    }
}
