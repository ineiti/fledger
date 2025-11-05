// This test creates a domain with a webpage in it.
// The setup uses 3 nodes to create the root domain,
// and a 4th node to add a subdomain to it, storing a
// webpage.
// Then a 5th node tries to read the webpage, given the
// subdomain.

use flarch::start_logging_filter_level;
use flcrypto::access::Condition;
use flmacro::test_async_stack;
use flmodules::{
    dht_storage::{
        core::{Cuckoo, RealmConfig},
        realm_view::{RealmView, RealmViewBuilder},
    },
    flo::blob::{FloBlobPage, FloBlobTag},
    testing::wallet::Wallet,
};

mod simul;
use simul::*;

const LOG_LVL: log::LevelFilter = log::LevelFilter::Info;

#[test_async_stack]
async fn page_simple() -> anyhow::Result<()> {
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
            .map(|n| n.wallet.get_badge_cond())
            .collect(),
    ));
    root_wallet.signer = Some(root_nodes[0].wallet.get_signer());
    let realm = root_wallet.get_realm();
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
    let root_tag = FloBlobTag::new(realm.realm_id(), Condition::Fail, "root_tag", None, &[])?;
    realm.edit_data_signers(
        root_wallet.get_badge_cond(),
        |r| r.set_service("tag", root_tag.flo_id()),
        &[&mut root_nodes[0].wallet.get_signer()],
    )?;
    root_nodes[0].dht_storage.store_flo(realm.flo().clone())?;

    log::info!("Creating 4th node and sub-tag");
    let mut node_4 = simul.new_node().await?;
    simul.sync_check(&realm).await?;

    let sub_tag = FloBlobTag::new(realm.realm_id(), Condition::Fail, "sub_tag", None, &[])?;
    node_4.dht_storage.store_flo(sub_tag.clone().into())?;
    simul.sync_check(&sub_tag).await?;

    log::info!("Storing webpage");
    let homepage = FloBlobPage::new(
        realm.realm_id(),
        node_4.wallet.get_badge_cond(),
        "/",
        INDEX_HTML.into(),
        None,
        &[&node_4.wallet.get_signer()],
    )?;
    realm.edit_data_signers(
        root_wallet.get_badge_cond(),
        |r| r.set_service("http", homepage.flo_id()),
        &[&mut root_nodes[0].wallet.get_signer()],
    )?;
    node_4.dht_storage.store_flo(homepage.clone().into())?;
    node_4.dht_storage.store_flo(realm.clone().into())?;
    simul.sync_check(&sub_tag).await?;
    simul.sync_check(&homepage).await?;
    simul.sync_check(&realm).await?;

    Ok(())
}

#[test_async_stack]
async fn page_full() -> anyhow::Result<()> {
    start_logging_filter_level(vec!["flmodules", "webpage"], log::LevelFilter::Debug);
    // start_logging_filter_level(vec!["flmodules::dht_storage", "webpage"], LOG_LVL);

    log::info!("Setting up root nodes, realm, root page, and root tag.");

    let mut simul = Simul::new().await?;
    let nbr_root = 2;
    let nbr_noise = 3;
    let mut root_nodes = simul.new_nodes(nbr_root).await?;

    let mut wallet_root = Wallet::new();
    wallet_root.badge_condition = Some(Condition::NofT(
        1,
        root_nodes
            .iter_mut()
            .map(|n| n.wallet.get_badge_cond())
            .collect(),
    ));

    let root_signers_val = vec![root_nodes[0].wallet.get_signer().clone()];

    let rv_builder = RealmViewBuilder::new(
        root_nodes[0].dht_storage.clone(),
        "root".to_string(),
        wallet_root.get_badge_cond(),
        root_signers_val.clone(),
    )
    .config(RealmConfig {
        max_space: 12e3 as u64,
        max_flo_size: 4e3 as u32,
    })
    .root_http(
        "fledger".to_string(),
        INDEX_HTML.to_string(),
        None,
        wallet_root.get_badge_cond(),
        root_signers_val.clone(),
    )
    .root_tag(
        "fledger".to_string(),
        None,
        wallet_root.get_badge_cond(),
        root_signers_val.clone(),
    );
    let rv_root = rv_builder.build().await?;
    let root_page = rv_root.get_root_page().expect("Getting root page");
    let root_tag = rv_root.get_root_tag().expect("Getting root tag");

    log::info!("Setting up noise nodes and fetching realm, page, and tag");
    let noise_nodes = simul.new_nodes(nbr_noise).await?;
    simul.log_connections();
    simul.sync_check(&rv_root.realm).await?;
    simul.sync_check(&root_tag).await?;
    simul.sync_check(&root_page).await?;

    let mut rv_noise = RealmView::new_first(noise_nodes[0].dht_storage.clone()).await?;
    assert!(rv_noise.pages.storage.len() > 0);
    assert!(rv_noise.tags.storage.len() > 0);
    assert_eq!(root_page.blob_id(), rv_noise.pages.root);
    assert_eq!(root_tag.blob_id(), rv_noise.tags.root);

    let cuckoo_nbr: usize = 3;
    let root_signers = &root_signers_val.iter().collect::<Vec<_>>();
    for nbr in 0..cuckoo_nbr {
        log::info!("Adding cuckoo page and tag #{nbr}");
        let cuckoo_page = rv_noise
            .create_http_cuckoo(
                &format!("cuckoo_{nbr}"),
                "<html><h1>Cuckoo".to_string(),
                None,
                wallet_root.get_badge_cond(),
                Cuckoo::Parent(root_page.flo_id()),
                root_signers,
            )
            .await?;
        let cuckoo_tag = rv_noise.create_tag_cuckoo(
            &format!("cuckoo_{nbr}"),
            None,
            wallet_root.get_badge_cond(),
            Cuckoo::Parent(root_tag.flo_id()),
            root_signers,
        )?;
        log::trace!("IDs: {} - {}", cuckoo_page.flo_id(), cuckoo_tag.flo_id());
        simul.sync_check(&cuckoo_tag).await?;
        simul.sync_check(&cuckoo_page).await?;
    }
    simul.sync_check(&rv_root.realm).await?;

    log::info!("New node, and checking cuckoos");
    let new_node = simul.new_node().await?;
    simul.log_connections();
    simul.sync_check(&rv_root.realm).await?;
    simul.sync_check_cuckoos(&root_page, cuckoo_nbr).await?;
    simul.sync_check_cuckoos(&root_tag, cuckoo_nbr).await?;
    let rv_new = RealmView::new_first(new_node.dht_storage.clone()).await?;
    assert_eq!(cuckoo_nbr + 1, rv_new.pages.storage.len());
    assert_eq!(cuckoo_nbr + 1, rv_new.tags.storage.len());

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
