#![allow(dead_code, unused)]

use flarch::data_storage::{DataStorage, DataStorageTemp};
use flmodules::{
    dht_router::{broker::DHTRouter, kademlia},
    dht_storage::{broker::DHTStorage, core::DHTConfig},
    flo::{flo::FloWrapper, realm::RealmID},
    nodeconfig::NodeConfig,
    router::broker::BrokerRouter,
    testing::{
        network_simul::{NetworkSimul, RouterNode},
        wallet::Wallet,
    },
    timer::{Timer, TimerMessage},
};
use itertools::Itertools;
use serde::{de::DeserializeOwned, Serialize};

#[derive(Clone)]
pub struct Node {
    pub _config: NodeConfig,
    pub router: BrokerRouter,
    pub wallet: Wallet,
    pub _dht_routing: DHTRouter,
    pub dht_storage: DHTStorage,
}

impl Node {
    pub async fn new(
        ds: Box<dyn DataStorage + Send>,
        timer: &mut Timer,
        router_node: RouterNode,
    ) -> anyhow::Result<Self> {
        let dht_router = DHTRouter::start(
            router_node.config.info.get_id(),
            router_node.router.clone(),
            timer,
            kademlia::Config::default(),
        )
        .await?;

        log::debug!("{} is new node", router_node.config.info.get_id());

        let mut wallet = Wallet::new();
        // Setting a random RealmID to initialize all the signers and conditions.
        wallet.set_realm_id(RealmID::rnd());

        Ok(Self {
            dht_storage: DHTStorage::start(
                ds,
                router_node.config.info.get_id(),
                DHTConfig {
                    realms: vec![],
                    owned: vec![],
                    timeout: 10,
                },
                dht_router.broker.clone(),
                timer,
            )
            .await?,
            _config: router_node.config,
            router: router_node.router,
            wallet,
            _dht_routing: dht_router,
        })
    }
}

pub struct Simul {
    pub nodes: Vec<Node>,
    pub steps: usize,
    pub simul: NetworkSimul,
    pub timer: Timer,
    pub realm_id: RealmID,
}

impl Simul {
    pub async fn new() -> anyhow::Result<Self> {
        Ok(Self {
            nodes: vec![],
            steps: 5,
            simul: NetworkSimul::new().await?,
            timer: Timer::simul(),
            realm_id: RealmID::zero(),
        })
    }

    pub async fn new_node_raw(&mut self) -> anyhow::Result<Node> {
        self.nodes.push(
            Node::new(
                Box::new(DataStorageTemp::new()),
                &mut self.timer,
                self.simul.new_node().await?,
            )
            .await?,
        );
        Ok(self.nodes.last().unwrap().clone())
    }

    pub async fn new_node(&mut self) -> anyhow::Result<Node> {
        let node = self.new_node_raw().await?;
        self.simul.send_node_info().await?;
        self.tick().await?;
        self.tick().await?;
        self.send_sync().await?;
        Ok(node)
    }

    pub async fn new_nodes(&mut self, nbr: usize) -> anyhow::Result<Vec<Node>> {
        let mut nodes = vec![];
        for _ in 0..nbr {
            nodes.push(self.new_node().await?);
        }
        Ok(nodes)
    }

    pub async fn new_nodes_raw(&mut self, nbr: usize) -> anyhow::Result<Vec<Node>> {
        let mut nodes = vec![];
        for _ in 0..nbr {
            nodes.push(self.new_node_raw().await?);
        }
        Ok(nodes)
    }

    pub async fn send_sync(&mut self) -> anyhow::Result<()> {
        for node in &mut self.nodes {
            node.dht_storage.sync()?;
        }
        self.settle().await
    }

    pub async fn _set_realm_id(&mut self, rid: RealmID) {
        for node in &mut self.nodes {
            node.wallet.set_realm_id(rid.clone());
        }
    }

    pub async fn _store_verifiers(&mut self) -> anyhow::Result<()> {
        for node in &mut self.nodes {
            node.dht_storage
                .store_flo(node.wallet.get_verifier_flo().into())?;
        }
        self.settle().await
    }

    pub async fn settle(&mut self) -> anyhow::Result<()> {
        self.simul.settle().await?;
        for node in &mut self.nodes {
            node.router.settle(vec![]).await?;
        }
        Ok(())
    }

    pub async fn tick(&mut self) -> anyhow::Result<()> {
        self.settle().await?;
        self.timer.broker.emit_msg_out(TimerMessage::Second)?;
        self.settle().await
    }

    pub fn log_connections(&self) {
        for node in &self.nodes {
            log::debug!(
                "{} connects to {}",
                node._config.info.get_id(),
                node._dht_routing
                    .stats
                    .borrow()
                    .all_nodes
                    .iter()
                    .map(|n| format!("{}", n))
                    .sorted()
                    .collect::<Vec<_>>()
                    .join(" - ")
            );
        }
    }

    pub async fn sync_check<T: Serialize + DeserializeOwned + Clone>(
        &mut self,
        fw: &FloWrapper<T>,
    ) -> anyhow::Result<usize> {
        let id = &fw.global_id();
        let version = fw.version();
        for step in 0..self.steps {
            self.send_sync().await?;
            let mut matches = 0;
            for node in &mut self.nodes {
                if node
                    .dht_storage
                    .get_flo::<T>(id)
                    .await
                    .map(|flo| flo.version() == version)
                    .unwrap_or(false)
                {
                    matches += 1
                }
            }
            let log_msg = format!(
                "{}: step {step} matches {matches}/{}",
                format!(
                    "version {}/{}/{}",
                    fw.flo().flo_type(),
                    fw.flo_id(),
                    fw.version()
                ),
                self.nodes.len()
            );
            if step == 0 {
                log::debug!("{log_msg}");
            } else {
                log::info!("{log_msg}");
            }
            if matches == self.nodes.len() {
                return Ok(step + 1);
            }
        }
        Err(anyhow::anyhow!("Didn't succeed to sync all"))
    }

    pub async fn sync_check_cuckoos<T: Serialize + DeserializeOwned + Clone>(
        &mut self,
        fw: &FloWrapper<T>,
        cuckoos: usize,
    ) -> anyhow::Result<usize> {
        let id = &fw.global_id();
        for step in 0..self.steps {
            self.send_sync().await?;
            let mut matches = 0;
            for node in &mut self.nodes {
                if node
                    .dht_storage
                    .get_cuckoos(id)
                    .await
                    .map(|num| num.len() == cuckoos)
                    .unwrap_or(false)
                {
                    matches += 1
                }
            }
            log::debug!(
                "{}: step {step} matches {matches}/{}",
                format!("version {}/{}", fw.flo().flo_type(), fw.version()),
                self.nodes.len()
            );
            if matches == self.nodes.len() {
                return Ok(step + 1);
            }
        }
        Err(anyhow::anyhow!("Didn't succeed to sync all"))
    }

    pub async fn store_all(&mut self) -> anyhow::Result<()> {
        for node in &mut self.nodes {
            node.wallet.set_realm_id(self.realm_id.clone());
            node.wallet.store(&mut node.dht_storage.broker)?;
        }
        self.send_sync().await?;
        Ok(())
    }
}
