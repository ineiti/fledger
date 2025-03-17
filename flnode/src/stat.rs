use std::collections::HashMap;

use flarch::{
    broker::{Broker, SubsystemHandler},
    nodeids::U256,
    platform_async_trait,
};
use flmodules::network::broker::{BrokerNetwork, NetworkConnectionState, NetworkIn, NetworkOut};
use tokio::sync::watch;

pub type NetStats = HashMap<U256, NetworkConnectionState>;

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
            if let NetworkOut::ConnectionState(state) = msg {
                self.stats.insert(state.id, state);
                self.tx
                    .clone()
                    .map(|tx| tx.send(self.stats.clone()).is_err().then(|| self.tx = None));
            }
        }
        vec![]
    }
}
