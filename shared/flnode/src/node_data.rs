use thiserror::Error;

use flmodules::{
    gossip_events::{
        broker::GossipBroker,
        events::{Category, Event},
    },
    ping::{broker::PingBroker, module::PingConfig},
    timer::TimerMessage,
    broker::{Broker, BrokerError},
};
use flnet::{
    config::{ConfigError, NodeConfig},
    network::{NetCall, NetworkMessage},
};
use flarch::{
    data_storage::{DataStorage, DataStorageBase, StorageError},
    tasks::now,
};

use crate::modules::{random::RandomBroker, stat::StatBroker};

pub const CONFIG_NAME: &str = "nodeConfig";

/// The NodeState is the shared global state of every node.
/// It must only be stored as Arc<Mutex<NodeState>>.
/// Theoretically there should never be a collision for the mutex,
/// as wasm is single-threaded and should not be pre-emptively
/// interrupted.
/// But that assumption might be false with regard to websocket-
/// and webrtc-callbacks.
pub struct NodeData {
    /// The node configuration
    pub node_config: NodeConfig,
    /// Storage to be used
    pub storage: Box<dyn DataStorageBase>,
    /// Network broker
    pub broker_net: Broker<NetworkMessage>,

    // Subsystem data
    /// Stores the connection data
    pub stat: StatBroker,
    /// Handles a random number of connections
    pub random: RandomBroker,
    /// Gossip-events sent and received
    pub gossip: GossipBroker,
    /// Pings all connected nodes and informs about failing nodes
    pub ping: PingBroker,

    gossip_storage: Box<dyn DataStorage>,
}

use bitflags::bitflags;
bitflags! {
    pub struct Brokers: u32 {
        const ENABLE_STAT = 0b1;
        const ENABLE_RAND = 0b10;
        const ENABLE_GOSSIP = 0b100;
        const ENABLE_PING = 0b1000;
        const ENABLE_ALL = 0b1111;
    }
}

const STORAGE_GOSSIP_EVENTS: &str = "gossip_events";

impl NodeData {
    pub async fn start(
        storage: Box<dyn DataStorageBase>,
        node_config: NodeConfig,
        broker_net: Broker<NetworkMessage>,
    ) -> Result<Self, NodeDataError> {
        Self::start_some(storage, node_config, broker_net, Brokers::ENABLE_ALL).await
    }

    pub async fn start_some(
        storage: Box<dyn DataStorageBase>,
        node_config: NodeConfig,
        broker_net: Broker<NetworkMessage>,
        brokers: Brokers,
    ) -> Result<Self, NodeDataError> {
        let id = node_config.info.get_id();
        let gossip_storage = storage.get("fledger");
        let mut nd = Self {
            storage,
            node_config,
            broker_net,
            stat: StatBroker::start(Broker::new()).await?,
            random: RandomBroker::start(id, Broker::new()).await?,
            gossip: GossipBroker::start(id, Broker::new()).await?,
            ping: PingBroker::start(PingConfig::default(), Broker::new()).await?,
            gossip_storage,
        };
        if brokers.contains(Brokers::ENABLE_RAND) {
            nd.random = RandomBroker::start(id, nd.broker_net.clone()).await?;
            if brokers.contains(Brokers::ENABLE_GOSSIP) {
                let mut gossip = GossipBroker::start(id, nd.random.broker.clone()).await?;
                Self::init_gossip(&mut gossip, &nd.gossip_storage, &nd.node_config).await?;
                nd.gossip = gossip;
            }
            if brokers.contains(Brokers::ENABLE_PING) {
                nd.ping =
                    PingBroker::start(PingConfig::default(), nd.random.broker.clone()).await?;
            }
        }
        if brokers.contains(Brokers::ENABLE_STAT) {
            nd.stat = StatBroker::start(nd.broker_net.clone()).await?;
        }

        Ok(nd)
    }

    async fn init_gossip(
        gossip: &mut GossipBroker,
        gossip_storage: &Box<dyn DataStorage>,
        node_config: &NodeConfig,
    ) -> Result<(), NodeDataError> {
        let gossip_msgs_str = gossip_storage.get(STORAGE_GOSSIP_EVENTS).unwrap();
        if !gossip_msgs_str.is_empty() {
            if let Err(e) = gossip.storage.set(&gossip_msgs_str) {
                log::warn!("Couldn't load gossip messages: {}", e);
            }
        }
        gossip
            .add_event(Event {
                category: Category::NodeInfo,
                src: node_config.info.get_id(),
                created: now(),
                msg: serde_yaml::to_string(&node_config.info)?,
            })
            .await?;
        Ok(())
    }

    pub async fn add_timer(&mut self, mut timer: Broker<TimerMessage>) {
        timer
            .forward(
                self.broker_net.clone(),
                Box::new(|msg| (msg == TimerMessage::Second).then(|| NetCall::Tick.into())),
            )
            .await;
        self.random.add_timer(timer.clone()).await;
        self.gossip.add_timer(timer.clone()).await;
        self.ping.add_timer(timer).await;
    }

    pub fn update(&mut self) {
        self.stat.update();
        self.random.update();
        self.gossip.update();
        self.ping.update();
    }

    pub async fn process(&mut self) -> Result<(), NodeDataError> {
        self.broker_net.process().await?;
        self.random.broker.process().await?;
        self.gossip.broker.process().await?;
        self.ping.broker.process().await?;
        self.update();
        self.gossip_storage
            .set(STORAGE_GOSSIP_EVENTS, &self.gossip.storage.get()?)?;
        Ok(())
    }

    pub fn get_config(storage: Box<dyn DataStorageBase>) -> Result<NodeConfig, NodeDataError> {
        let client = "unknown";

        // New config place
        let mut storage_node = storage.get("fledger");

        let config_str = match storage_node.get(CONFIG_NAME) {
            Ok(s) => s,
            Err(_) => {
                log::info!("Couldn't load configuration - start with empty");
                "".to_string()
            }
        };
        let mut config = NodeConfig::try_from(config_str)?;
        config.info.client = client.to_string();
        storage_node.set(CONFIG_NAME, &config.to_string()?)?;
        log::info!(
            "Starting node: {} = {}",
            config.info.name,
            config.info.get_id()
        );
        Ok(config)
    }
}

#[derive(Debug, Error)]
pub enum NodeDataError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Broker(#[from] BrokerError),
    #[error(transparent)]
    Serde(#[from] serde_yaml::Error),
}

#[cfg(test)]
mod tests {
    use flmodules::gossip_events::{
        events::{Category, Event},
        module::GossipIn,
    };
    use flarch::{data_storage::TempDSB, start_logging};

    use super::*;

    #[tokio::test]
    async fn test_storage() -> Result<(), Box<dyn std::error::Error>> {
        start_logging();

        let storage = TempDSB::new();
        let nc = NodeConfig::new();
        let mut nd = NodeData::start(storage.clone(), nc.clone(), Broker::new()).await?;
        let event = Event {
            category: Category::TextMessage,
            src: nc.info.get_id(),
            created: 0.,
            msg: "something".into(),
        };
        nd.gossip
            .broker
            .emit_msg(GossipIn::AddEvent(event.clone()).into())
            .await?;
        nd.process().await?;

        let nd2 = NodeData::start(storage.clone(), nc.clone(), Broker::new()).await?;
        let events = nd2.gossip.storage.events(Category::TextMessage);
        assert_eq!(1, events.len());
        assert_eq!(&event, events.get(0).unwrap());
        Ok(())
    }
}
