// This test creates a domain with a webpage in it.
// The setup uses 3 nodes to create the root domain,
// and a 4th node to add a subdomain to it, storing a
// webpage.
// Then a 5th node tries to read the webpage, given the
// subdomain.

use flarch::{
    broker::BrokerError,
    data_storage::{DataStorage, DataStorageTemp},
    start_logging_filter_level,
};
use flcrypto::access::Condition;
use flmodules::{
    dht_router::{broker::DHTRouter, kademlia},
    dht_storage::{
        broker::DHTStorage,
        core::{Cuckoo, DHTConfig, RealmConfig},
        realm_view::RealmView,
    },
    flo::{
        blob::{FloBlobPage, FloBlobTag},
        crypto::Rules,
        flo::FloWrapper,
        realm::{FloRealm, RealmID},
    },
    nodeconfig::NodeConfig,
    router::broker::BrokerRouter,
    testing::{
        flo::Wallet,
        network_simul::{NetworkSimul, RouterNode},
    },
    timer::{Timer, TimerMessage},
};
use serde::{de::DeserializeOwned, Serialize};
use std::error::Error;

#[derive(Clone)]
struct Node {
    _config: NodeConfig,
    router: BrokerRouter,
    wallet: Wallet,
    _dht_routing: DHTRouter,
    dht_storage: DHTStorage,
}

impl Node {
    async fn new(
        ds: Box<dyn DataStorage + Send>,
        timer: &mut Timer,
        router_node: RouterNode,
    ) -> Result<Self, Box<dyn Error>> {
        let dht_router = DHTRouter::start(
            router_node.config.info.get_id(),
            router_node.router.clone(),
            timer,
            kademlia::Config::default(),
        )
        .await?;

        log::debug!("{} is new node", router_node.config.info.get_id());

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
            wallet: Wallet::new(),
            _dht_routing: dht_router,
        })
    }
}

struct Simul {
    nodes: Vec<Node>,
    steps: usize,
    simul: NetworkSimul,
    timer: Timer,
    realm_id: RealmID,
}

impl Simul {
    async fn new() -> Result<Self, BrokerError> {
        Ok(Self {
            nodes: vec![],
            steps: 2,
            simul: NetworkSimul::new().await?,
            timer: Timer::simul(),
            realm_id: RealmID::zero(),
        })
    }

    async fn new_node(&mut self) -> Result<Node, Box<dyn Error>> {
        self.nodes.push(
            Node::new(
                Box::new(DataStorageTemp::new()),
                &mut self.timer,
                self.simul.new_node().await?,
            )
            .await?,
        );
        self.simul.send_node_info().await?;
        self.tick().await?;
        self.send_sync().await?;
        Ok(self.nodes.last().unwrap().clone())
    }

    async fn new_nodes(&mut self, nbr: usize) -> Result<Vec<Node>, Box<dyn Error>> {
        let mut nodes = vec![];
        for _ in 0..nbr {
            nodes.push(self.new_node().await?);
        }
        Ok(nodes)
    }

    async fn send_sync(&mut self) -> Result<(), BrokerError> {
        for node in &mut self.nodes {
            node.dht_storage.sync()?;
        }
        self.settle().await
    }

    async fn settle(&mut self) -> Result<(), BrokerError> {
        self.simul.settle().await?;
        for node in &mut self.nodes {
            node.router.settle(vec![]).await?;
        }
        Ok(())
    }

    async fn tick(&mut self) -> Result<(), BrokerError> {
        self.settle().await?;
        self.timer.broker.emit_msg_out(TimerMessage::Second)?;
        self.settle().await
    }

    async fn sync_check<T: Serialize + DeserializeOwned + Clone>(
        &mut self,
        fw: &FloWrapper<T>,
    ) -> Result<usize, Box<dyn Error>> {
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
            log::info!(
                "{}: step {step} matches {matches}/{}",
                format!("version {}/{}", fw.flo().flo_type(), fw.version()),
                self.nodes.len()
            );
            if matches == self.nodes.len() {
                return Ok(step + 1);
            }
        }
        Err("Didn't succeed to sync all".into())
    }

    async fn sync_check_cuckoos<T: Serialize + DeserializeOwned + Clone>(
        &mut self,
        fw: &FloWrapper<T>,
        cuckoos: usize,
    ) -> Result<usize, Box<dyn Error>> {
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
            log::info!(
                "{}: step {step} matches {matches}/{}",
                format!("version {}/{}", fw.flo().flo_type(), fw.version()),
                self.nodes.len()
            );
            if matches == self.nodes.len() {
                return Ok(step + 1);
            }
        }
        Err("Didn't succeed to sync all".into())
    }

    async fn store_all(&mut self) -> Result<(), Box<dyn Error>> {
        for node in &mut self.nodes {
            node.wallet.set_realm_id(self.realm_id.clone());
            node.wallet.store(&mut node.dht_storage.broker)?;
        }
        self.send_sync().await?;
        Ok(())
    }
}

const LOG_LVL: log::LevelFilter = log::LevelFilter::Info;

#[tokio::test]
async fn run_test() -> Result<(), Box<dyn Error>> {
    // start_logging_filter_level(vec![], LOG_LVL);
    start_logging_filter_level(vec!["flmodules::dht_storage", "webpage"], LOG_LVL);
    let mut simul = Simul::new().await?;
    let root_nodes_nbr = 3;
    let noise_nodes_nbr = 10;

    let mut root_nodes = simul.new_nodes(root_nodes_nbr).await?;

    log::info!("Creating the realm");
    let mut root_wallet = Wallet::new();

    // let every root-node handle the domain as it sees fit.
    root_wallet.badge_condition = Some(Condition::NofT(
        1,
        root_nodes
            .iter_mut()
            .map(|n| n.wallet.get_badge_condition())
            .collect(),
    ));
    let mut realm = FloRealm::new(
        "root",
        root_wallet.get_badge_rules(),
        RealmConfig {
            max_space: 1e6 as u64,
            max_flo_size: 1e4 as u32,
        },
    )?;
    root_wallet.set_realm_id(realm.realm_id());
    root_nodes[0].dht_storage.store_flo(realm.flo().clone())?;
    simul.realm_id = realm.realm_id();
    simul.settle().await?;

    log::info!("Adding {noise_nodes_nbr} nodes");
    let mut noise_nodes = vec![];
    for _ in 0..noise_nodes_nbr {
        noise_nodes.push(simul.new_node().await?);
    }
    simul.store_all().await?;

    log::info!("Setting up all node ids and start synching");
    simul.sync_check(&realm).await?;

    log::info!("Creating a Tag");
    let root_tag = FloBlobTag::new(realm.realm_id(), Rules::None, "root_tag", None)?;
    realm.edit_sign_update(
        |r| r.set_service("tag", root_tag.flo_id()),
        root_wallet.get_badge_condition(),
        realm.rules().clone(),
        vec![],
        &[&*root_nodes[0].wallet.get_signer()],
    )?;
    root_nodes[0].dht_storage.store_flo(realm.flo().clone())?;

    log::info!("Creating 4th node and sub-tag");
    let mut node_4 = simul.new_node().await?;
    simul.sync_check(&realm).await?;

    let sub_tag = FloBlobTag::new(realm.realm_id(), Rules::None, "sub_tag", None)?;
    node_4.dht_storage.store_flo(sub_tag.clone().into())?;
    simul.sync_check(&sub_tag).await?;

    log::info!("Storing webpage");
    let homepage = FloBlobPage::new(
        realm.realm_id(),
        node_4.wallet.get_rules(),
        "/",
        INDEX_HTML.into(),
        None,
    )?;
    realm.edit_sign_update(
        |r| r.set_service("http", homepage.flo_id()),
        root_wallet.get_badge_condition(),
        realm.rules().clone(),
        vec![],
        &[&*root_nodes[0].wallet.get_signer()],
    )?;
    node_4.dht_storage.store_flo(homepage.clone().into())?;
    node_4.dht_storage.store_flo(realm.clone().into())?;
    simul.sync_check(&sub_tag).await?;
    simul.sync_check(&homepage).await?;
    simul.sync_check(&realm).await?;

    Ok(())
}

#[tokio::test]
async fn test_realm_view() -> Result<(), Box<dyn Error>> {
    // start_logging_filter_level(vec![], LOG_LVL);
    start_logging_filter_level(vec!["flmodules::dht_storage", "webpage"], LOG_LVL);

    log::info!("Setting up root nodes, realm, root page, and root tag.");

    let mut simul = Simul::new().await?;
    let nbr_root = 3;
    let nbr_noise = 10;
    let mut root_nodes = simul.new_nodes(nbr_root).await?;

    let mut wallet_root = Wallet::new();
    wallet_root.badge_condition = Some(Condition::NofT(
        1,
        root_nodes
            .iter_mut()
            .map(|n| n.wallet.get_badge_condition())
            .collect(),
    ));

    let mut rv_root = RealmView::new_create_realm_config(
        root_nodes[0].dht_storage.clone(),
        "root",
        wallet_root.get_badge_rules(),
        RealmConfig {
            max_space: 6e3 as u64,
            max_flo_size: 3e3 as u32,
        },
    )
    .await?;

    let root_http = rv_root.create_http("fledger", INDEX_HTML.to_string(), None, None)?;
    let signers = vec![&*(root_nodes[0].wallet.signer.as_deref().unwrap())];
    rv_root.set_realm_http(root_http.flo_id(), &signers).await?;
    let root_tag = rv_root.create_tag("fledger", None, None)?;
    rv_root.set_realm_tag(root_tag.flo_id(), &signers).await?;

    log::info!("Setting up noise nodes and fetching realm, page, and tag");
    let noise_nodes = simul.new_nodes(nbr_noise).await?;
    simul.sync_check(&root_tag).await?;
    simul.sync_check(&root_http).await?;
    simul.sync_check(&rv_root.realm).await?;

    let mut rv_noise = RealmView::new(noise_nodes[0].dht_storage.clone()).await?;
    assert!(rv_noise.pages.len() > 0);
    assert!(rv_noise.tags.len() > 0);

    let cuckoo_nbr: usize = 3;
    for nbr in 0..cuckoo_nbr {
        log::info!("Adding cuckoo page and tag #{nbr}");
        let cuckoo_page = rv_noise.create_http_cuckoo(
            &format!("cuckoo_{nbr}"),
            "<html><h1>Cuckoo".to_string(),
            None,
            None,
            Cuckoo::Parent(rv_noise.pages[0].flo_id()),
        )?;
        let cuckoo_tag = rv_noise.create_tag_cuckoo(
            &format!("cuckoo_{nbr}"),
            None,
            None,
            Cuckoo::Parent(rv_noise.tags[0].flo_id()),
        )?;
        log::trace!("IDs: {} - {}", cuckoo_page.flo_id(), cuckoo_tag.flo_id());
        simul.sync_check(&cuckoo_tag).await?;
        simul.sync_check(&cuckoo_page).await?;
    }
    simul.sync_check(&rv_root.realm).await?;

    log::info!("New node, and checking cuckoos");
    let new_node = simul.new_node().await?;
    simul
        .sync_check_cuckoos(&rv_noise.pages.first().unwrap(), cuckoo_nbr)
        .await?;
    simul
        .sync_check_cuckoos(&rv_noise.tags.first().unwrap(), cuckoo_nbr)
        .await?;
    let rv_new = RealmView::new(new_node.dht_storage.clone()).await?;
    assert_eq!(cuckoo_nbr + 1, rv_new.pages.len());
    assert_eq!(cuckoo_nbr + 1, rv_new.tags.len());

    Ok(())
}

const INDEX_HTML: &str = r##"
<!DOCTYPE html>
<html>
  <head>
    <title>Fledger</title>
  </head>
<body>
<h1>Fledger</h1>

Fast, Fun, Fair Ledger, or Fledger puts the <strong>FUN</strong> back in blockchain!
</body>
</html>
    "##;
