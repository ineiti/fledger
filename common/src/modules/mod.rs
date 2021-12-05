use crate::types::U256;

pub mod random_connections;

pub type NodeID = U256;

#[derive(Debug, PartialEq, Clone)]
pub struct NodeIDs(Vec<NodeID>);

impl NodeIDs {
    /// Returns a NodeIDs with 'nbr' random NodeID inside.
    pub fn new(nbr: u32) -> Self {
        let mut ret = NodeIDs { 0: vec![] };
        for _ in 0..nbr {
            ret.0.push(NodeID::rnd())
        }
        ret
    }

    /// Returns a new NodeIDs with a copy of the NodeIDs from..from+len
    pub fn slice(&self, from: usize, len: usize) -> Self {
        NodeIDs {
            0: self.0[from..(from + len)].to_vec(),
        }
    }

    /// Goes through all nodes in the structure and removes the ones that
    /// are NOT in the argument.
    /// It returns the removed nodes.
    pub fn remove_missing(&mut self, nodes: &NodeIDs) -> NodeIDs {
        self.remove(nodes, false)
    }

    /// Goes through all nodes in the structure and removes the ones that
    /// are in the argument.
    /// It returns the removed nodes.
    pub fn remove_existing(&mut self, nodes: &NodeIDs) -> NodeIDs {
        self.remove(nodes, true)
    }

    fn remove(&mut self, nodes: &NodeIDs, exists: bool) -> NodeIDs {
        let mut ret = vec![];
        let mut i = 0;
        while i < self.0.len() {
            if nodes.0.contains(&self.0[i]) == exists {
                ret.push(self.0.remove(i));
            } else {
                i += 1;
            }
        }
        NodeIDs(ret)
    }

    // pub fn clone(&self) -> NodeIDs {
    //     NodeIDs(self.0.iter().cloned().collect())
    // }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Error;

    #[test]
    fn test_new_nodes() -> Result<(), Error> {
        let mut nodes = NodeIDs::new(4);

        let nodes2 = nodes.remove_missing(&NodeIDs::new(0));
        assert_eq!(0, nodes.0.len());
        assert_eq!(4, nodes2.0.len());

        Ok(())
    }
}