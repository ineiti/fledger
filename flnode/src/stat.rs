use std::collections::HashMap;

use flarch::{
    broker::{Broker, SubsystemHandler},
    nodeids::{NodeID, U256},
    platform_async_trait,
};
use flmodules::{
    network::broker::{BrokerNetwork, NetworkConnectionState, NetworkIn, NetworkOut},
    nodeconfig::NodeInfo,
};
use tokio::sync::watch;

#[derive(Default, Debug, Clone)]
pub struct NetStats {
    pub states: HashMap<U256, NetworkConnectionState>,
    pub connected: Vec<NodeID>,
    pub online: Vec<NodeID>,
    pub node_infos: HashMap<NodeID, NodeInfo>,
}

/// Collects the statistics of the connections sent by the network broker.
pub struct NetworkStats {
    stats: NetStats,
    tx: Option<watch::Sender<NetStats>>,
}

impl NetworkStats {
    pub async fn start(mut broker_net: BrokerNetwork) -> anyhow::Result<watch::Receiver<NetStats>> {
        let stats = NetStats::default();
        let (tx, rx) = watch::channel(stats.clone());
        let mut broker = Broker::new();
        broker
            .add_handler(Box::new(Self {
                stats,
                tx: Some(tx),
            }))
            .await?;
        broker_net
            .add_translator_o_ti(broker, Box::new(|msg| Some(msg)))
            .await?;
        Ok(rx)
    }
}

#[platform_async_trait()]
impl SubsystemHandler<NetworkOut, NetworkIn> for NetworkStats {
    async fn messages(&mut self, msgs: Vec<NetworkOut>) -> Vec<NetworkIn> {
        for msg in msgs {
            match msg {
                NetworkOut::NodeListFromWS(node_infos) => {
                    self.stats.online = node_infos.iter().map(|ni| ni.get_id()).collect();
                    for info in node_infos {
                        self.stats.node_infos.insert(info.get_id(), info);
                    }
                }
                NetworkOut::ConnectionState(state) => {
                    self.stats.states.insert(state.id, state);
                }
                NetworkOut::Connected(id) => {
                    self.stats.connected.push(id);
                }
                NetworkOut::Disconnected(id) => {
                    self.stats.connected.retain(|i| i != &id);
                }
                _ => {}
            }
        }
        self.tx
            .clone()
            .map(|tx| tx.send(self.stats.clone()).is_err().then(|| self.tx = None));
        vec![]
    }
}

impl NetStats {
    pub fn node_infos_connected(&self) -> Vec<NodeInfo> {
        self.connected
            .iter()
            .filter_map(|id| self.node_infos.get(id))
            .cloned()
            .collect()
    }
}
