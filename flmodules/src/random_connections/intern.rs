use flcrypto::tofrombytes::ToFromBytes;
use itertools::concat;
use serde::{Deserialize, Serialize};

use flarch::{
    broker::SubsystemHandler,
    nodeids::{NodeID, U256},
    platform_async_trait,
};
use tokio::sync::watch;

use crate::{
    network::broker::{NetworkIn, NetworkOut, MODULE_NAME},
    router::messages::NetworkWrapper,
};

use super::{
    broker::{RandomIn, RandomOut},
    core::RandomStats,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModuleMessage {
    Module(NetworkWrapper),
    DropConnection,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum InternIn {
    Tick,
    Random(RandomIn),
    Network(NetworkOut),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum InternOut {
    Random(RandomOut),
    Network(NetworkIn),
}

/// RandomConnections listens for new available nodes and then chooses
/// to randomly connect to a set number of nodes.
#[derive(Debug)]
pub(super) struct Intern {
    id: U256,
    cfg: Config,
    storage: RandomStats,
    tx: Option<watch::Sender<RandomStats>>,
    fill: u32,
}

impl Intern {
    pub fn new(id: U256) -> (Self, watch::Receiver<RandomStats>) {
        let storage = RandomStats::default();
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

    fn msg_tick(&mut self) -> Vec<InternOut> {
        self.storage.tick();

        concat([
            self.new_connection(),
            // self.need_drop(),
            // self.churn(),
            self.fill_connection(),
            self.update(),
        ])
    }

    fn msg_rand(&mut self, msg: RandomIn) -> Vec<InternOut> {
        match msg {
            RandomIn::NodeFailure(node) => {
                self.storage.failure(&(&vec![node]).into());
                concat([
                    self.new_connection(),
                    // self.need_drop(),
                    self.disconnect_msg(node),
                ])
            }
            RandomIn::NetworkWrapperToNetwork(dst, msg) => {
                if self.storage.connected.contains(&dst) {
                    Self::create_network_msg(dst, ModuleMessage::Module(msg))
                } else {
                    self.storage.disconnect((&vec![dst]).into());
                    log::trace!(
                        "{self:p} Dropping message with len {} to unconnected node {dst} - making sure list is updated", msg.to_rmp_bytes().len()
                    );
                    self.connected_msg()
                }
            }
        }
    }

    fn msg_net(&mut self, msg: NetworkOut) -> Vec<InternOut> {
        match msg {
            NetworkOut::MessageFromNode(id, msg_nw) => msg_nw
                .unwrap_yaml::<ModuleMessage>(MODULE_NAME)
                .map(|msg_mod| self.network_msg(id, msg_mod))
                .unwrap_or(vec![]),
            NetworkOut::NodeListFromWS(list) => {
                let filtered = list
                    .into_iter()
                    .filter(|ni| ni.get_id() != self.id)
                    .collect();
                self.storage.new_infos(filtered);
                self.new_connection()
            }
            NetworkOut::Connected(id) => {
                self.storage.connect((&vec![id]).into());
                concat([
                    // self.need_drop(),
                    self.connected_msg(),
                ])
            }
            NetworkOut::Disconnected(id) => {
                self.storage.disconnect((&vec![id]).into());
                concat([self.new_connection(), self.connected_msg()])
            }
            _ => vec![],
        }
    }

    fn disconnect_msg(&self, dst: NodeID) -> Vec<InternOut> {
        concat([
            self.connected_msg(),
            vec![InternOut::Network(NetworkIn::Disconnect(dst))],
        ])
    }

    fn connected_msg(&self) -> Vec<InternOut> {
        vec![
            InternOut::Random(RandomOut::NodeIDsConnected(
                self.storage.connected.get_nodes(),
            )),
            InternOut::Random(RandomOut::NodeInfosConnected(
                self.storage.get_connected_info(),
            )),
        ]
    }

    /// Processes one message from the network.
    fn network_msg(&mut self, id: U256, msg: ModuleMessage) -> Vec<InternOut> {
        match msg {
            ModuleMessage::Module(msg_mod) => {
                self.storage.connect((&vec![id]).into());
                vec![InternOut::Random(RandomOut::NetworkWrapperFromNetwork(
                    id, msg_mod,
                ))]
            }
            ModuleMessage::DropConnection => {
                self.storage.disconnect((&vec![id]).into());
                concat([
                    vec![InternOut::Network(NetworkIn::Disconnect(id))],
                    self.new_connection(),
                ])
            }
        }
    }

    /// Fills up to ceil(nodes_needed / 2) nodes by emitting ConnectNode.
    fn new_connection(&mut self) -> Vec<InternOut> {
        self.storage
            .fill_up()
            .0
            .into_iter()
            .map(|n| InternOut::Network(NetworkIn::Connect(n)))
            .collect()
    }

    fn fill_connection(&mut self) -> Vec<InternOut> {
        self.fill += 1;
        if self.fill >= self.cfg.fill_connected {
            self.fill = 0;
            if self.storage.total_len() < self.storage.nodes_needed() {
                return self
                    .storage
                    .choose_new(1)
                    .0
                    .into_iter()
                    .map(|n| InternOut::Network(NetworkIn::Connect(n)))
                    .collect();
            }
        }
        vec![]
    }

    fn update(&mut self) -> Vec<InternOut> {
        self.tx.clone().map(|tx| {
            tx.send(self.storage.clone())
                .is_err()
                .then(|| self.tx = None)
        });
        self.connected_msg()
    }

    fn create_network_msg(dst: NodeID, msg: ModuleMessage) -> Vec<InternOut> {
        NetworkWrapper::wrap_yaml(MODULE_NAME, &msg)
            .map(|msg_nw| vec![InternOut::Network(NetworkIn::MessageToNode(dst, msg_nw))])
            .unwrap_or(vec![])
    }
}

#[platform_async_trait()]
impl SubsystemHandler<InternIn, InternOut> for Intern {
    async fn messages(&mut self, msgs: Vec<InternIn>) -> Vec<InternOut> {
        msgs.into_iter()
            .flat_map(|msg| match msg {
                InternIn::Tick => self.msg_tick(),
                InternIn::Random(random_in) => self.msg_rand(random_in),
                InternIn::Network(network_out) => self.msg_net(network_out),
            })
            .collect()
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

impl Default for Config {
    fn default() -> Self {
        Config {
            churn_connected: 60 * 60,
            connecting_timeout: 10,
            fill_connected: 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use flarch::{nodeids::NodeIDs, start_logging};

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
        let mut rc = Intern::new(nodes[0].get_id()).0;
        let reply = rc.msg_net(NetworkOut::NodeListFromWS(nodes));
        log::debug!("{reply:?}");

        Ok(())
    }
}
