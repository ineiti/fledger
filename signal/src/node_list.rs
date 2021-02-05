use common::config::NodeInfo;
use common::types::U256;

/// Information held by the REST service to allow other nodes to connect.
/// TODO: allow to connect to any node.
#[derive(Debug, Clone)]
pub struct NodeList {
    pub idle_ids: Vec<U256>,
    pub nodes: Vec<NodeInfo>,
}

/// Limit maximum number of IDs in the queue.
const MAX_NEW_IDS: usize = 10;

impl NodeList {
    pub fn new() -> NodeList {
        NodeList {
            idle_ids: Vec::new(),
            nodes: Vec::new(),
        }
    }

    pub fn clear_nodes(&mut self) {
        println!("Clearing all nodes");
        self.nodes.clear();
    }

    pub fn get_nodes(&self) -> Vec<NodeInfo> {
        return (&self.nodes).into_iter().map(|x| x.clone()).collect();
    }

    pub fn get_new_idle(&mut self) -> U256 {
        let u = U256::rnd();
        while self.idle_ids.len() >= MAX_NEW_IDS {
            self.idle_ids.pop();
        }
        self.idle_ids.push(u.clone());
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

        if (&self.nodes)
            .into_iter()
            .find(|x| x.public == p.node.public)
            .is_none()
        {
            println!("Adding node {:?}", p.node);
            self.nodes.push(p.node);
        }

        return Ok(());
    }
}

#[cfg(test)]
mod tests {

    use common::config::NodeInfo;
    use common::rest::PostWebRTC;

    use super::NodeList;
    use common::types::U256;

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

    /// Check all ways to fail and succeed to add a node.
    #[test]
    fn add_node() {
        // Refuse to add a node without a list_id
        let mut nl = NodeList::new();
        let n1 = NodeInfo::new();
        assert_eq!(
            true,
            nl.add_node(PostWebRTC {
                list_id: U256::rnd(),
                node: n1.clone(),
            })
            .is_err()
        );
        assert_eq!(0, nl.nodes.len());

        // Create a list_id, but don't use it
        let id1 = nl.get_new_idle();
        assert_eq!(
            true,
            nl.add_node(PostWebRTC {
                list_id: U256::rnd(),
                node: n1.clone(),
            })
            .is_err()
        );
        assert_eq!(0, nl.nodes.len());

        // Use the list_id to add a node
        assert_eq!(
            true,
            nl.add_node(PostWebRTC {
                list_id: id1.clone(),
                node: n1.clone(),
            })
            .is_ok()
        );
        assert_eq!(1, nl.nodes.len());

        // Cannot add a node or a new node with an existing list_id
        assert_eq!(
            false,
            nl.add_node(PostWebRTC {
                list_id: id1.clone(),
                node: n1.clone(),
            })
            .is_ok()
        );
        assert_eq!(1, nl.nodes.len());
        let n2 = NodeInfo::new();
        assert_eq!(
            false,
            nl.add_node(PostWebRTC {
                list_id: id1.clone(),
                node: n2.clone(),
            })
            .is_ok()
        );
        assert_eq!(1, nl.nodes.len());

        // Cannot add the same node twice - add_node returns true, but doesn't add it.
        let id2 = nl.get_new_idle();
        assert_eq!(
            true,
            nl.add_node(PostWebRTC {
                list_id: id2.clone(),
                node: n1.clone(),
            })
            .is_ok()
        );
        assert_eq!(1, nl.nodes.len());

        // Now id2 is used up, and node2 cannot be added
        assert_eq!(
            false,
            nl.add_node(PostWebRTC {
                list_id: id2.clone(),
                node: n2.clone(),
            })
            .is_ok()
        );
        assert_eq!(1, nl.nodes.len());

        // Add a second node
        let id3 = nl.get_new_idle();
        assert_eq!(
            true,
            nl.add_node(PostWebRTC {
                list_id: id3,
                node: n2.clone(),
            })
            .is_ok()
        );
        assert_eq!(2, nl.nodes.len());

        // Verify nodes are correct
        let nodes = nl.get_nodes();
        assert_eq!(&n1, nodes.get(0).unwrap());
        assert_eq!(&n2, nodes.get(1).unwrap());
    }
}
