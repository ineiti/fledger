use itertools::concat;
use serde::{Deserialize, Serialize};

use flarch::nodeids::{NodeID, NodeIDs, U256};

use crate::{nodeconfig::NodeInfo, overlay::messages::NetworkWrapper};

use super::core::RandomStorage;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModuleMessage {
    Module(NetworkWrapper),
    DropConnection,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RandomMessage {
    Input(RandomIn),
    Output(RandomOut),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RandomIn {
    NodeList(Vec<NodeInfo>),
    NodeFailure(NodeID),
    NodeConnected(NodeID),
    NodeDisconnected(NodeID),
    NodeCommFromNetwork(NodeID, ModuleMessage),
    NetworkWrapperToNetwork(NodeID, NetworkWrapper),
    Tick,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RandomOut {
    ConnectNode(NodeID),
    DisconnectNode(NodeID),
    NodeIDsConnected(NodeIDs),
    NodeInfosConnected(Vec<NodeInfo>),
    NodeCommToNetwork(NodeID, ModuleMessage),
    NetworkWrapperFromNetwork(NodeID, NetworkWrapper),
    Storage(RandomStorage),
}

/// RandomConnections listens for new available nodes and then chooses
/// to randomly connect to a set number of nodes.
#[derive(Debug)]
pub struct RandomConnections {
    cfg: Config,
    pub storage: RandomStorage,
    fill: u32,
}

impl RandomConnections {
    pub fn new(cfg: Config) -> Self {
        RandomConnections {
            cfg,
            storage: RandomStorage::default(),
            fill: 0,
        }
    }

    /// Processes one message and returns messages that need to be treated by the
    /// system.
    pub fn process_message(&mut self, msg: RandomIn) -> Vec<RandomOut> {
        let out = match msg {
            RandomIn::NodeList(nodes) => {
                self.storage.new_infos(nodes);
                self.new_connection()
            }
            RandomIn::NodeConnected(node) => {
                self.storage.connect((&vec![node]).into());
                self.need_drop()
            }
            RandomIn::NodeDisconnected(node) => {
                self.storage.disconnect((&vec![node]).into());
                self.new_connection()
            }
            RandomIn::NodeFailure(node) => {
                self.storage.failure(&(&vec![node]).into());
                concat([
                    vec![RandomOut::DisconnectNode(node)],
                    self.new_connection(),
                    self.need_drop(),
                ])
            }
            RandomIn::Tick => {
                self.storage.tick();

                concat([
                    self.new_connection(),
                    self.need_drop(),
                    self.churn(),
                    self.fill_connection(),
                    self.update(),
                ])
            }
            RandomIn::NodeCommFromNetwork(id, node_msg) => self.network_msg(id, node_msg),
            RandomIn::NetworkWrapperToNetwork(dst, msg) => {
                if self.storage.connected.contains(&dst) {
                    vec![RandomOut::NodeCommToNetwork(
                        dst,
                        ModuleMessage::Module(msg),
                    )]
                } else {
                    log::warn!(
                        "{self:p} Dropping message to unconnected node {dst} - making sure we're disconnected"
                    );
                    vec![
                        RandomOut::DisconnectNode(dst),
                        RandomOut::NodeIDsConnected(self.storage.connected.get_nodes()),
                    ]
                }
            }
        };
        out
    }

    /// Processes one message from the network.
    pub fn network_msg(&mut self, id: U256, msg: ModuleMessage) -> Vec<RandomOut> {
        match msg {
            ModuleMessage::Module(msg_mod) => vec![RandomOut::NetworkWrapperFromNetwork(id, msg_mod)],
            ModuleMessage::DropConnection => {
                self.storage.disconnect((&vec![id]).into());
                concat([vec![RandomOut::DisconnectNode(id)], self.new_connection()])
            }
        }
    }

    /// Returns a clone of the connected NodeIDs.
    pub fn connected(&self) -> NodeIDs {
        self.storage.connected.get_nodes()
    }

    /// Fills up to ceil(nodes_needed / 2) nodes by emitting ConnectNode.
    fn new_connection(&mut self) -> Vec<RandomOut> {
        self.storage
            .fill_up()
            .0
            .into_iter()
            .map(|n| RandomOut::ConnectNode(n))
            .collect()
    }

    /// Asks other nodes to drop the connection if there are too many connections.
    /// First asks nodes that need to be churned out, then the latest nodes to join.
    fn need_drop(&mut self) -> Vec<RandomOut> {
        let drop: Vec<RandomOut> = self
            .storage
            .limit_active(self.cfg.churn_connected)
            .0
            .into_iter()
            .flat_map(|n| {
                vec![
                    RandomOut::NodeCommToNetwork(n, ModuleMessage::DropConnection),
                    RandomOut::DisconnectNode(n),
                ]
            })
            .collect();
        if drop.len() > 0 {
            log::debug!("Dropping connections: {drop:?}");
        }
        // drop
        vec![]
    }

    fn churn(&mut self) -> Vec<RandomOut> {
        // self.storage
        //     .choose_new(
        //         self.storage
        //             .connected
        //             .count_expired(self.cfg.churn_connected),
        //     )
        //     .0
        //     .into_iter()
        //     .map(|n| RandomOut::ConnectNode(n))
        //     .collect()
        vec![]
    }

    fn fill_connection(&mut self) -> Vec<RandomOut> {
        self.fill += 1;
        if self.fill >= self.cfg.fill_connected {
            self.fill = 0;
            if self.storage.total_len() < self.storage.nodes_needed() {
                return self
                    .storage
                    .choose_new(1)
                    .0
                    .into_iter()
                    .map(|n| RandomOut::ConnectNode(n))
                    .collect();
            }
        }
        vec![]
    }

    fn update(&self) -> Vec<RandomOut> {
        vec![
            RandomOut::NodeIDsConnected(self.storage.connected.get_nodes()),
            RandomOut::NodeInfosConnected(self.storage.get_connected_info()),
            RandomOut::Storage(self.storage.clone()),
        ]
    }
}

/// All intervals are indicated in ticks.
#[derive(Debug)]
pub struct Config {
    /// How many ticks a node stays in the list before it is
    /// possibly replaced by another node.
    pub churn_connected: u32,

    /// How long a node stays in the connecting-queue before it is
    /// considered to have a failed connection.
    pub connecting_timeout: u32,

    /// If the system doesn't have the needed number of connections,
    /// every fill_connected interval one additional node will be
    /// connected.
    pub fill_connected: u32,
}

impl Config {
    pub fn default() -> Self {
        Config {
            churn_connected: 60 * 60,
            connecting_timeout: 10,
            fill_connected: 10,
        }
    }
}

impl From<RandomIn> for RandomMessage {
    fn from(msg: RandomIn) -> RandomMessage {
        RandomMessage::Input(msg)
    }
}

impl From<RandomOut> for RandomMessage {
    fn from(msg: RandomOut) -> RandomMessage {
        RandomMessage::Output(msg)
    }
}

#[cfg(test)]
mod tests {
    use flarch::start_logging;

    use crate::{nodeconfig::NodeConfig, random_connections::nodes::Nodes};

    use super::*;
    use core::fmt::Error;

    fn _make_nodes(n: usize) -> Nodes {
        let mut nodes = Nodes::new();
        nodes.add_new(NodeIDs::new(n as u32).0);
        for node in 0..n {
            nodes.0[node].ticks = node as u32 + 1;
        }
        nodes
    }

    // Tests the nodes and the remove methods
    #[test]
    fn test_update_list() -> Result<(), Error> {
        start_logging();

        let nodes = vec![NodeConfig::new().info];
        let mut rc = RandomConnections::new(Config::default());
        let reply = rc.process_message(RandomIn::NodeList(nodes));
        log::debug!("{reply:?}");

        Ok(())
    }
}
