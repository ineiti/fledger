use flcrypto::tofrombytes::ToFromBytes;
use itertools::concat;
use serde::{Deserialize, Serialize};

use flarch::{
    broker::SubsystemHandler,
    nodeids::{NodeID, NodeIDs, U256},
    platform_async_trait,
};
use tokio::sync::watch;

use crate::router::messages::NetworkWrapper;

use super::{
    broker::{RandomIn, RandomOut},
    core::RandomStorage,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModuleMessage {
    Module(NetworkWrapper),
    DropConnection,
}

/// RandomConnections listens for new available nodes and then chooses
/// to randomly connect to a set number of nodes.
#[derive(Debug)]
pub struct Messages {
    id: U256,
    cfg: Config,
    storage: RandomStorage,
    tx: Option<watch::Sender<RandomStorage>>,
    fill: u32,
}

impl Messages {
    pub fn new(id: U256) -> (Self, watch::Receiver<RandomStorage>) {
        let storage = RandomStorage::default();
        let (tx, rx) = watch::channel(storage.clone());
        (
            Self {
                id,
                cfg: Config::default(),
                storage,
                tx: Some(tx),
                fill: 0,
            },
            rx,
        )
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
                concat([
                    // self.need_drop(),
                    self.connected_msg(),
                ])
            }
            RandomIn::NodeDisconnected(node) => {
                self.storage.disconnect((&vec![node]).into());
                concat([self.new_connection(), self.connected_msg()])
            }
            RandomIn::NodeFailure(node) => {
                self.storage.failure(&(&vec![node]).into());
                concat([
                    self.new_connection(),
                    // self.need_drop(),
                    self.disconnect_msg(node),
                ])
            }
            RandomIn::Tick => {
                self.storage.tick();

                concat([
                    self.new_connection(),
                    // self.need_drop(),
                    // self.churn(),
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
                    self.storage.disconnect((&vec![dst]).into());
                    log::trace!(
                        "{self:p} Dropping message with len {} to unconnected node {dst} - making sure list is updated", msg.to_rmp_bytes().len()
                    );
                    self.connected_msg()
                }
            }
        };
        out
    }

    fn disconnect_msg(&self, dst: NodeID) -> Vec<RandomOut> {
        concat([self.connected_msg(), vec![RandomOut::DisconnectNode(dst)]])
    }

    fn connected_msg(&self) -> Vec<RandomOut> {
        vec![
            RandomOut::NodeIDsConnected(self.storage.connected.get_nodes()),
            RandomOut::NodeInfosConnected(self.storage.get_connected_info()),
        ]
    }

    /// Processes one message from the network.
    pub fn network_msg(&mut self, id: U256, msg: ModuleMessage) -> Vec<RandomOut> {
        match msg {
            ModuleMessage::Module(msg_mod) => {
                self.storage.connect((&vec![id]).into());
                vec![RandomOut::NetworkWrapperFromNetwork(id, msg_mod)]
            }
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
    fn _need_drop(&mut self) -> Vec<RandomOut> {
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
            log::trace!("Dropping connections because there are too many: {drop:?}");
        }
        drop
    }

    fn _churn(&mut self) -> Vec<RandomOut> {
        self.storage
            .choose_new(
                self.storage
                    .connected
                    .count_expired(self.cfg.churn_connected),
            )
            .0
            .into_iter()
            .map(|n| RandomOut::ConnectNode(n))
            .collect()
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

    fn update(&mut self) -> Vec<RandomOut> {
        self.tx.clone().map(|tx| {
            tx.send(self.storage.clone())
                .is_err()
                .then(|| self.tx = None)
        });
        self.connected_msg()
    }
}

#[platform_async_trait()]
impl SubsystemHandler<RandomIn, RandomOut> for Messages {
    async fn messages(&mut self, msgs: Vec<RandomIn>) -> Vec<RandomOut> {
        let mut out = vec![];
        for msg in msgs {
            log::trace!("{} processing {msg:?}", self.id);
            out.extend(self.process_message(msg));
        }

        out
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

#[cfg(test)]
mod tests {
    use flarch::start_logging;

    use crate::{nodeconfig::NodeConfig, random_connections::nodes::Nodes};

    use super::*;

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
    fn test_update_list() -> anyhow::Result<()> {
        start_logging();

        let nodes = vec![NodeConfig::new().info];
        let mut rc = Messages::new(nodes[0].get_id()).0;
        let reply = rc.process_message(RandomIn::NodeList(nodes));
        log::debug!("{reply:?}");

        Ok(())
    }
}
