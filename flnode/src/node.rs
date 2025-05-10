use log::{error, info};
use std::collections::HashMap;
use thiserror::Error;
use tokio::sync::watch;

use flarch::{broker::BrokerError, nodeids::NodeID};
use flarch::{
    data_storage::{DataStorage, StorageError},
    tasks::now,
};
use flmodules::{
    dht_router::{broker::DHTRouter, kademlia},
    dht_storage::{self, broker::DHTStorage, core::DHTConfig},
    flo::storage::CryptoStorage,
    gossip_events::{
        broker::Gossip,
        core::{self, Category},
    },
    network::broker::{BrokerNetwork, NetworkError, NetworkIn},
    nodeconfig::{ConfigError, NodeConfig, NodeInfo},
    ping::broker::Ping,
    random_connections::broker::RandomBroker,
    router::broker::{BrokerRouter, RouterNetwork, RouterRandom},
    timer::Timer,
    web_proxy::{
        broker::{WebProxy, WebProxyError},
        core::WebProxyConfig,
    },
    Modules,
};

use crate::stat::{NetStats, NetworkStats};

#[derive(Error, Debug)]
pub enum NodeError {
    #[error("Couldn't get lock")]
    Lock,
    #[error("Missing subsystem {0}")]
    Missing(String),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Network(#[from] NetworkError),
    #[error(transparent)]
    Broker(#[from] BrokerError),
    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),
    #[error(transparent)]
    WebProxy(#[from] WebProxyError),
    #[error(transparent)]
    DHTStorage(#[from] dht_storage::broker::StorageError),
}

/// The node structure holds it all together. It is the main structure of the project.
pub struct Node {
    /// The node configuration
    pub node_config: NodeConfig,
    /// Storage to be used
    pub storage: Box<dyn DataStorage + Send>,
    /// Network broker
    pub broker_net: BrokerNetwork,
    /// Network IO broker
    pub network_io: BrokerRouter,
    /// Timer broker
    pub timer: Timer,
    /// Storing all the signers and ACEs
    pub crypto_storage: CryptoStorage,

    // Subsystem data
    /// Stores the connection data
    pub stat: Option<watch::Receiver<NetStats>>,
    /// Handles a random number of connections
    pub random: Option<RandomBroker>,
    /// Gossip-events sent and received
    pub gossip: Option<Gossip>,
    /// Pings all connected nodes and informs about failing nodes
    pub ping: Option<Ping>,
    /// Answers GET requests from another node
    pub webproxy: Option<WebProxy>,
    /// Sets up a dht routing system using Kademlia
    pub dht_router: Option<DHTRouter>,
    /// Decentralized storage system using DHTRouting
    pub dht_storage: Option<DHTStorage>,
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Node")
            .field("node_config", &self.node_config)
            .field("storage", &self.storage)
            .field("broker_net", &self.broker_net)
            .field("network_io", &self.network_io)
            .field("timer", &self.timer)
            .field("stat", &self.stat)
            .field("dht_storage", &self.dht_storage)
            .finish()
    }
}

const STORAGE_CONFIG: &str = "nodeConfig";

impl Node {
    /// Create new node by loading the config from the storage.
    /// This also initializes the network and starts listening for
    /// new messages from the signalling server and from other nodes.
    /// The actual logic is handled in Logic.
    pub async fn start(
        storage: Box<dyn DataStorage + Send>,
        node_config: NodeConfig,
        broker_net: BrokerNetwork,
    ) -> anyhow::Result<Self> {
        info!(
            "Starting node: {} = {}",
            node_config.info.name,
            node_config.info.get_id()
        );

        let mut timer = Timer::start().await?;
        timer
            .tick_second(broker_net.clone(), NetworkIn::Tick)
            .await?;
        let network_io = RouterNetwork::start(broker_net.clone()).await?;

        let modules = node_config.info.modules;
        let id = node_config.info.get_id();
        let mut random = None;
        let mut gossip = None;
        let ping = None;
        let mut webproxy = None;
        let mut stat = None;
        let mut dht_router = None;
        let mut dht_storage = None;
        if modules.contains(Modules::RAND) {
            let rnd = RandomBroker::start(id, broker_net.clone(), &mut timer).await?;
            if modules.contains(Modules::GOSSIP) {
                gossip = Some(
                    Gossip::start(
                        storage.clone(),
                        node_config.info.clone(),
                        rnd.broker.clone(),
                        &mut timer,
                    )
                    .await?,
                );
            }
            if modules.contains(Modules::PING) {
                // log::warn!("Ping is disabled");
                // ping =
                //     Some(Ping::start(PingConfig::default(), rnd.broker.clone(), &mut timer).await?);
            }
            if modules.contains(Modules::WEBPROXY) {
                webproxy = Some(
                    WebProxy::start(
                        storage.clone_box(),
                        id,
                        RouterRandom::start(rnd.broker.clone()).await?,
                        WebProxyConfig::default(),
                    )
                    .await?,
                );
            }
            random = Some(rnd);
        }
        if modules.contains(Modules::STAT) {
            stat = Some(NetworkStats::start(broker_net.clone()).await?)
        }
        if modules.contains(Modules::DHT_ROUTER) {
            let routing = DHTRouter::start(
                id,
                network_io.clone(),
                &mut timer,
                kademlia::Config::default(),
            )
            .await?;
            if modules.contains(Modules::DHT_STORAGE) {
                dht_storage = Some(
                    DHTStorage::start(
                        storage.clone_box(),
                        id,
                        DHTConfig::default(),
                        routing.broker.clone(),
                        &mut timer,
                    )
                    .await?,
                );
            }
            dht_router = Some(routing);
        }

        Ok(Self {
            crypto_storage: CryptoStorage::new(storage.clone()),
            storage,
            node_config,
            broker_net,
            network_io,
            timer,
            stat,
            random,
            gossip,
            ping,
            webproxy,
            dht_router,
            dht_storage,
        })
    }

    /// Requests a list of all connected nodes
    pub async fn request_list(&mut self) -> anyhow::Result<()> {
        self.broker_net
            .emit_msg_in(NetworkIn::WSUpdateListRequest)?;
        Ok(())
    }

    /// Returns all NodeInfos that are stored locally. All ids that do not have a
    /// corresponding NodeInfo in the local storage are dropped.
    pub fn nodes_info(&self, ids: Vec<NodeID>) -> anyhow::Result<Vec<NodeInfo>> {
        let mut nodeinfos = self.nodes_info_all()?;
        Ok(ids.iter().filter_map(|id| nodeinfos.remove(&id)).collect())
    }

    /// Gets the current list of connected nodes - these are the nodes that this node is
    /// currently connected to, and can be shorter than the list of all nodes in the system.
    pub fn nodes_connected(&self) -> anyhow::Result<Vec<NodeInfo>> {
        if let Some(r) = self.random.as_ref() {
            return self.nodes_info(r.storage.borrow().connected.get_nodes().0);
        }
        Err(NodeError::Missing("Random".into()).into())
    }

    /// Returns all currently online nodes in the whole system. Every node will only connect
    /// to a subset of these nodes, which can be get with `nodes_connected`.
    pub fn nodes_online(&self) -> anyhow::Result<Vec<NodeInfo>> {
        if let Some(r) = self.random.as_ref() {
            return self.nodes_info(r.storage.borrow().known.0.clone());
        }
        Err(NodeError::Missing("Random".into()).into())
    }

    /// Returns a list of known nodes from the local storage
    pub fn nodes_info_all(&self) -> anyhow::Result<HashMap<NodeID, NodeInfo>> {
        if let Some(g) = self.gossip.as_ref() {
            let events = g.events(Category::NodeInfo);

            let mut nodeinfos = HashMap::new();
            for ni in events {
                // For some reason I cannot get it to work the from_str in a .iter().map()
                match NodeInfo::decode(&ni.msg) {
                    Ok(info) => {
                        nodeinfos.insert(info.get_id(), info);
                    }
                    Err(e) => log::error!("Parse-error {e:?} for {}", ni.msg),
                }
            }
            Ok(nodeinfos)
        } else {
            Err(NodeError::Missing("Gossip".into()).into())
        }
    }

    /// Adds a new chat message that will be broadcasted to the system.
    pub async fn add_chat_message(&mut self, msg: String) -> anyhow::Result<()> {
        if let Some(g) = self.gossip.as_mut() {
            let event = core::Event {
                category: core::Category::TextMessage,
                src: self.node_config.info.get_id(),
                created: now(),
                msg,
            };
            g.add_event(event).await?;
            Ok(())
        } else {
            Err(NodeError::Missing("Gossip".into()).into())
        }
    }

    /// Static method

    /// Fetches the config
    pub fn get_config(storage: Box<dyn DataStorage>) -> anyhow::Result<NodeConfig> {
        let config_str = match storage.get(STORAGE_CONFIG) {
            Ok(s) => s,
            Err(_) => {
                log::info!("Couldn't load configuration - start with empty");
                "".to_string()
            }
        };
        let mut config = NodeConfig::decode(&config_str);
        #[cfg(target_family = "wasm")]
        let enable_webproxy_request = false;
        // Only unix based clients can send http GET requests.
        #[cfg(target_family = "unix")]
        let enable_webproxy_request = true;

        config
            .info
            .modules
            .set(Modules::WEBPROXY_REQUESTS, enable_webproxy_request);
        Self::set_config(storage, &config.encode())?;
        Ok(config)
    }

    /// Updates the config of the node
    pub fn set_config(mut storage: Box<dyn DataStorage>, config: &str) -> anyhow::Result<()> {
        storage.set(STORAGE_CONFIG, config)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use flarch::{
        broker::Broker, data_storage::DataStorageTemp, start_logging, start_logging_filter_level,
    };
    use flmodules::gossip_events::{
        core::{Category, Event},
        messages::GossipIn,
    };

    use super::*;

    #[tokio::test]
    async fn test_storage() -> anyhow::Result<()> {
        start_logging();

        let storage = DataStorageTemp::new();
        let nc = NodeConfig::new();
        let mut nd = Node::start(storage.clone_box(), nc.clone(), Broker::new()).await?;
        let event = Event {
            category: Category::TextMessage,
            src: nc.info.get_id(),
            created: 0,
            msg: "something".into(),
        };
        nd.gossip
            .as_mut()
            .unwrap()
            .broker
            .settle_msg_in(GossipIn::AddEvent(event.clone()).into())
            .await?;

        let nd2 = Node::start(storage.clone_box(), nc.clone(), Broker::new()).await?;
        let events = nd2
            .gossip
            .unwrap()
            .storage
            .borrow()
            .events(Category::TextMessage);
        assert_eq!(1, events.len());
        assert_eq!(&event, events.get(0).unwrap());
        Ok(())
    }

    #[tokio::test]
    async fn test_store_node() -> anyhow::Result<()> {
        start_logging_filter_level(vec![], log::LevelFilter::Info);

        let mut node = Node::start(
            Box::new(DataStorageTemp::new()),
            NodeConfig::new(),
            Broker::new(),
        )
        .await?;
        node.gossip.as_mut().unwrap().broker.settle(vec![]).await?;

        log::debug!(
            "storage is: {:?}",
            node.gossip.as_ref().unwrap().storage.borrow()
        );
        assert_eq!(1, node.gossip.unwrap().events(Category::NodeInfo).len());
        Ok(())
    }
}
