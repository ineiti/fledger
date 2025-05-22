use flarch::start_logging_filter_level;
use flcrypto::{access::Condition, tofrombytes::ToFromBytes};
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
    let nbr_nodes = 20;
    let max_space = 20_000;
    let html_size = 1500;
    let prob_no_fail = 0.99;

    let mut simul = Simul::new().await?;
    let mut nodes = simul.new_nodes_raw(nbr_nodes - 1).await?;
    let mut root = simul.new_nodes(1).await?.get(0).unwrap().clone();
    nodes.push(root.clone());

    let rv_builder = RealmViewBuilder::new(
        root.dht_storage.clone(),
        "root".to_string(),
        Condition::Pass,
        vec![],
    )
    .config(RealmConfig {
        max_space: max_space as u64,
        max_flo_size: 10e3 as u32,
    })
    .root_http(
        "fledger".to_string(),
        "a".repeat(html_size),
        None,
        Condition::Pass,
        vec![],
    )
    .root_tag("fledger".to_string(), None, Condition::Pass, vec![]);
    let mut rv_root = rv_builder.build().await?;
    let root_page = rv_root.get_root_page().expect("Getting root page").clone();
    let root_tag = rv_root.get_root_tag().expect("Getting root tag");
    let flo_size = root_page.size();

    log::info!("Waiting for realm to be propagated");
    simul.sync_check(&rv_root.realm).await?;

    // TODO: improve calculation of this value:
    // - cuckoos take up space - the more pages, the more cuckoo-IDs are stored
    // - calculation difference in FloStorage <-> Flo
    let flo_per_node =
        (max_space - root_page.size() - root_tag.size() - rv_root.realm.size()) / flo_size;
    let (total_flos, p) = (|| {
        let mut total_flos: usize = nbr_nodes * flo_per_node;
        loop {
            let p = (1f64
                - (1f64 - (flo_per_node as f64) / (total_flos as f64)).powi(nbr_nodes as i32))
            .powi(total_flos as i32);
            if p > prob_no_fail {
                return (total_flos, p);
            }
            total_flos -= 1;
        }
    })();
    log::info!("Writing {total_flos} pages to fill storage - Probability of success: {p} for flo/node={flo_per_node}");
    let mut page_ids = vec![];
    for nbr in 0..total_flos {
        let page = rv_root
            .create_http_cuckoo(
                &format!("cuckoo_{nbr}"),
                "a".repeat(html_size),
                None,
                Condition::Pass,
                Cuckoo::Parent(root_page.flo_id()),
                &[],
            )
            .await?;
        simul.sync_check(&page).await?;
        page_ids.push(page.flo_id());

        let mut id_found = vec![];
        for id in &page_ids {
            let mut found = 0;
            for node in &mut nodes {
                let flos = node.dht_storage.get_flos().await?;
                if flos.iter().any(|f| f.flo_id() == *id) {
                    found += 1;
                }
            }
            id_found.push(found);
        }
        log::debug!("Ids found in this many nodes BEFORE sync: {:?}", id_found);

        simul.send_sync().await?;
        simul.send_sync().await?;
        simul.send_sync().await?;

        let mut id_found = vec![];
        for id in &page_ids {
            let mut found = 0;
            for node in &mut nodes {
                let flos = node.dht_storage.get_flos().await?;
                if flos.iter().any(|f| f.flo_id() == *id) {
                    found += 1;
                }
            }
            id_found.push(found);
        }
        log::info!("Ids found in this many nodes AFTER sync: {:?}", id_found);
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
    for id in &page_ids {
        log::debug!("Getting page {id}");
        if root
            .dht_storage
            .get_flo_timeout::<BlobPage>(&rid.global_id(id.clone()), 1000)
            .await
            .is_err()
        {
            log::warn!("Failed to fetch page {id}");
            for node in &mut nodes {
                let flos = node.dht_storage.get_flos().await?;
                if flos.iter().any(|f| &f.flo_id() == id) {
                    log::warn!("Page {id} found in node!");
                    if root
                        .dht_storage
                        .get_flo_timeout::<BlobPage>(&rid.global_id(id.clone()), 1000)
                        .await
                        .is_ok()
                    {
                        log::warn!("And now the page is found also with get_flo");
                    }
                    break;
                }
            }
        }
    }

    log::info!("Getting all pages from all nodes and verifying if they were accessible locally.");
    for node in &mut nodes {
        let local_ids = node.dht_storage.get_flos().await?;
        log::trace!(
            "Node {} fetches all {} pages, and has {} local page_ids",
            node._config.info.get_id(),
            page_ids.len(),
            local_ids.len() - 3
        );
        for id in &page_ids {
            let is_local = local_ids.iter().any(|i| &i.flo_id() == id);

            // Get the page from the DHT
            if node
                .dht_storage
                .get_flo::<BlobPage>(&rid.global_id(id.clone()))
                .await
                .is_ok()
            {
                let local_ids = node.dht_storage.get_flos().await?;
                if is_local != local_ids.iter().any(|i| &i.flo_id() == id) {
                    log::warn!(
                        "Node {} got {id} inverted from is_local = {is_local}",
                        node._config.info.get_id()
                    );
                }
            }
        }
    }

    Ok(())
}
