use std::cmp::{max, min};

use itertools::Itertools;
use rand::prelude::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::nodeconfig::NodeInfo;

use super::nodes::Nodes;
use flarch::nodeids::{NodeIDs, U256};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RandomStorage {
    pub connected: Nodes,
    pub connecting: Nodes,
    pub known: NodeIDs,
    pub infos: Vec<NodeInfo>,
}

impl Default for RandomStorage {
    fn default() -> Self {
        Self {
            connected: Nodes::new(),
            connecting: Nodes::new(),
            known: NodeIDs::empty(),
            infos: vec![],
        }
    }
}

impl RandomStorage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_list(&mut self, list: NodeIDs) {
        self.known.merge(list);
    }

    pub fn new_infos(&mut self, list: Vec<NodeInfo>) {
        self.new_list(
            list.iter()
                .map(|ni| ni.get_id())
                .collect::<Vec<U256>>()
                .into(),
        );
        self.infos = self
            .infos
            .iter()
            .chain(list.iter())
            .unique()
            .cloned()
            .collect();
    }

    pub fn get_connected_info(&self) -> Vec<NodeInfo> {
        self.infos
            .iter()
            .filter(|ni| self.connected.contains(&ni.get_id()))
            .cloned()
            .collect()
    }

    pub fn connect(&mut self, nodes: NodeIDs) {
        self.connected.add_new(nodes.clone().0);
        self.connecting.remove(&nodes.clone().into());
        self.new_list(nodes);
    }

    pub fn disconnect(&mut self, nodes: NodeIDs) {
        self.known.remove_existing(&nodes);
        self.connected.remove(&nodes);
        self.connecting.remove(&nodes);
    }

    pub fn connecting(&mut self, nodes: NodeIDs) {
        self.connecting.add_new(nodes.clone().0);
        self.connected.remove(&nodes);
        self.new_list(nodes);
    }

    pub fn total_len(&self) -> usize {
        self.connected.0.len() + self.connecting.0.len()
    }

    pub fn failure(&mut self, nodes: &NodeIDs) {
        self.connecting.remove(nodes);
        self.connected.remove(nodes);
        self.known.remove_existing(nodes);
    }

    pub fn tick(&mut self) {
        self.connected.tick();
        self.connecting.tick();
    }

    pub fn choose_new(&mut self, mut nodes: usize) -> NodeIDs {
        let mut unused = self.known.clone();
        unused.remove_existing(&self.connected.get_nodes());
        unused.remove_existing(&self.connecting.get_nodes());
        nodes = min(nodes, unused.0.len());
        let connecting = NodeIDs {
            0: unused
                .0
                .choose_multiple(&mut rand::thread_rng(), nodes)
                .cloned()
                .collect(),
        };
        self.connecting(connecting.clone());
        connecting
    }

    pub fn fill_up(&mut self) -> NodeIDs {
        let needed = (self.nodes_needed() as i32 + 1) / 2 - self.total_len() as i32;
        self.choose_new(max(0, needed) as usize)
    }

    pub fn limit_active(&mut self, churn: u32) -> NodeIDs {
        let max_connected = self.nodes_needed() * 2;
        let mut ids = NodeIDs::empty();
        if self.total_len() > max_connected {
            ids.merge(self.connected.oldest_ticks(churn));
        }
        if self.total_len() - ids.0.len() > max_connected {
            ids.merge(self.connected.keep_oldest_n(max_connected + ids.0.len()))
        }
        self.disconnect(ids.clone());
        ids
    }

    /// Returns the number of nodes needed to have a high probability of a
    /// fully connected network.
    /// This structure will initiate at least half of these connections, and supposes
    /// that the other half will come from other modules.
    /// Once the amount of connected nodes reaches 2 * nodes_needed, new connections
    /// will not be accepted anymore.
    pub fn nodes_needed(&self) -> usize {
        let nodes = self.known.0.len();
        match nodes {
            0 | 1 => nodes,
            _ => ((nodes as f64).ln() * 2.).ceil() as usize,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list() {
        let nodes = NodeIDs::new(4);
        let mut s = RandomStorage::default();
        s.new_list(nodes.slice(0, 2));
        assert_eq!(2, s.known.0.len());
        s.new_list(nodes.slice(1, 2));
        assert_eq!(3, s.known.0.len());
    }

    #[test]
    fn test_connect() {
        let nodes = NodeIDs::new(4);
        let mut s = RandomStorage::default();

        s.new_list(nodes.slice(0, 1));
        s.connect(nodes.slice(0, 2));
        assert_eq!(2, s.known.0.len());
        assert_eq!(2, s.connected.0.len());
    }

    #[test]
    fn choose_new() {
        let nodes = NodeIDs::new(40);
        let mut s = RandomStorage::default();

        s.new_list(nodes.clone());
        s.connecting(nodes.slice(0, 10));
        s.connect(nodes.slice(10, 10));
        let added = s.choose_new(30);
        assert_eq!(20, added.0.len());
        assert!(nodes.slice(20, 20).contains_all(&added));
    }
}
