use crate::{config::NodeInfo, rest::PostWebRTC};

use crate::types::U256;

/// Information held by the REST service to allow other nodes to connect.
/// TODO: allow to connect to any node.
#[derive(Debug, Clone)]
pub struct NodeList {
    pub idle_ids: Vec<U256>,
    pub nodes: Vec<NodeInfo>,
}

impl NodeList {
    pub fn new() -> NodeList {
        NodeList {
            idle_ids: Vec::new(),
            nodes: Vec::new(),
        }
    }

    pub fn get_nodes(&self) -> Vec<NodeInfo> {
        return (&self.nodes).into_iter().map(|x| x.clone()).collect();
    }

    pub fn get_new_idle(&mut self) -> U256 {
        let u: U256 = rand::random();
        self.idle_ids.push(u);
        return u;
    }

    /// Adds a new node to the list of active nodes.
    /// The given list_id must be in self.idle_ids, else the new node will be rejected.
    pub fn add_node(&mut self, p: PostWebRTC) -> Result<(), String> {
        let mut found = false;
        for i in 0..self.idle_ids.len() {
            if self.idle_ids[i] == p.list_id {
                self.idle_ids.remove(i);
                found = true;
                break;
            }
        }
        if !found {
            return Err("bad id".to_string());
        }

        if (&self
            .nodes)
            .into_iter()
            .find(|x| x.public == p.node.public)
            .is_none()
        {
            self.nodes.push(p.node);
        }

        return Ok(());
    }
}

#[cfg(test)]
mod tests {

    use super::NodeList;
    use crate::config::NodeInfo;

    #[test]
    fn get_new_idle() {
        let mut nl = NodeList::new();
        assert_eq!(0, nl.idle_ids.len());

        let i1 = nl.get_new_idle();
        assert_eq!(1, nl.idle_ids.len());
        assert_eq!(i1, i1);
        assert_eq!(true, i1 == i1);

        let i2 = nl.get_new_idle();
        assert_eq!(2, nl.idle_ids.len());
        assert_ne!(i1, i2);
        assert_eq!(false, i1 == i2);
    }

    #[test]
    fn add_node() {
        let nl = NodeList::new();
        let n1 = NodeInfo::new();
        let n2 = NodeInfo::new();
    }
}
