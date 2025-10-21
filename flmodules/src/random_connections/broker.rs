use flarch::{
    add_translator_direct, add_translator_link,
    broker::Broker,
    nodeids::{NodeID, NodeIDs, U256},
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::{
    network::broker::BrokerNetwork,
    nodeconfig::NodeInfo,
    random_connections::{
        core::RandomStats,
        intern::{Intern, InternIn, InternOut},
    },
    router::messages::NetworkWrapper,
    timer::{BrokerTimer, Timer},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RandomIn {
    NodeFailure(NodeID),
    NetworkWrapperToNetwork(NodeID, NetworkWrapper),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RandomOut {
    NodeIDsConnected(NodeIDs),
    NodeInfosConnected(Vec<NodeInfo>),
    NetworkWrapperFromNetwork(NodeID, NetworkWrapper),
}

pub type BrokerRandom = Broker<RandomIn, RandomOut>;

pub struct RandomBroker {
    pub broker: BrokerRandom,
    pub stats: watch::Receiver<RandomStats>,
}

impl RandomBroker {
    pub async fn start(
        id: U256,
        timer: BrokerTimer,
        network: BrokerNetwork,
    ) -> anyhow::Result<Self> {
        let (int, stats) = Intern::new(id);
        let mut intern = Broker::new_with_handler(Box::new(int)).await?.0;

        Timer::second(timer, intern.clone(), InternIn::Tick).await?;
        add_translator_link!(intern, network, InternIn::Network, InternOut::Network);
        let broker = Broker::new();
        add_translator_direct!(intern, broker.clone(), InternIn::Random, InternOut::Random);

        Ok(Self { stats, broker })
    }
}
