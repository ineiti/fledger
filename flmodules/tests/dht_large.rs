use flarch::start_logging_filter_level;
use flcrypto::access::Condition;
use flmacro::test_async_stack;
use flmodules::{
    dht_storage::{
        core::{Cuckoo, RealmConfig},
        realm_view::RealmViewBuilder,
    },
    flo::blob::BlobPage,
};

mod simul;
use simul::*;

#[test_async_stack]
async fn dht_large() -> anyhow::Result<()> {
    start_logging_filter_level(vec!["flmodules", "dht_large"], log::LevelFilter::Info);

    log::info!("Setting up nodes.");

    let mut simul = Simul::new().await?;
    let nbr_nodes = 10;
    let mut nodes = simul.new_nodes_raw(nbr_nodes).await?;
    let mut root = simul.new_nodes(1).await?.get(0).unwrap().clone();

    let max_space = 12_000;
    let html_size = 1500;
    let rv_builder = RealmViewBuilder::new(
        root.dht_storage.clone(),
        "root".to_string(),
        Condition::Pass,
        vec![],
    )
    .config(RealmConfig {
        max_space: max_space as u64,
        max_flo_size: 4e3 as u32,
    })
    .root_http(
        "fledger".to_string(),
        INDEX_HTML.to_string(),
        None,
        Condition::Pass,
        vec![],
    )
    .root_tag("fledger".to_string(), None, Condition::Pass, vec![]);
    let mut rv_root = rv_builder.build().await?;
    let root_page = rv_root.get_root_page().expect("Getting root page").clone();

    log::info!("Waiting for realm to be propagated");
    simul.sync_check(&rv_root.realm).await?;

    let cuckoo_nbr: usize = nbr_nodes * max_space / html_size / 24;
    log::info!("Writing {cuckoo_nbr} pages to fill storage");
    let mut page_ids = vec![];
    for nbr in 0..cuckoo_nbr {
        let cuckoo_page = rv_root
            .create_http_cuckoo(
                &format!("cuckoo_{nbr}"),
                "a".repeat(1500),
                None,
                Condition::Pass,
                Cuckoo::Parent(root_page.flo_id()),
                &[],
            )
            .await?;
        simul.sync_check(&cuckoo_page).await?;
        page_ids.push(cuckoo_page.flo_id());
    }
    simul.sync_check(&rv_root.realm).await?;
    let realm = rv_root.update_realm().await?;
    log::info!("Realm version is: {}", realm.flo().version());

    log::info!("Check number of stored pages in each node");
    let rid = rv_root.realm.realm_id();
    let flo_nbrs = nodes
        .iter()
        .map(|node| {
            node.dht_storage
                .stats
                .borrow()
                .realm_stats
                .get(&rid)
                .unwrap()
                .flos
                .to_string()
        })
        .collect::<Vec<_>>()
        .join(" - ");
    log::info!("Flo distribution: {}", flo_nbrs);

    let mut max_flos = 0;
    let mut max_node_idx = 0;
    for (idx, node) in nodes.iter().enumerate() {
        let flos_count = node
            .dht_storage
            .stats
            .borrow()
            .realm_stats
            .get(&rid)
            .unwrap()
            .flos;
        if flos_count > max_flos {
            max_flos = flos_count;
            max_node_idx = idx;
        }
    }

    log::info!("Maximum numbers of flos: {}", max_flos);
    let node = nodes.get_mut(max_node_idx).unwrap();
    log::info!("Stats: {:?}", node.dht_storage.stats.borrow());
    log::debug!("Flos: {:?}", node.dht_storage.get_flos().await?);

    log::info!("Fetching all pages from root");
    for id in page_ids {
        log::debug!("Getting page {id}");
        if root
            .dht_storage
            .get_flo::<BlobPage>(&rid.global_id(id.clone()))
            .await
            .is_err()
        {
            log::warn!("Failed to fetch page {id}");
            for node in &mut nodes {
                let flos = node.dht_storage.get_flos().await?;
                if flos.iter().any(|f| f.flo_id() == id) {
                    log::warn!("Page {id} found in node!");
                    break;
                }
            }
        }
    }

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
