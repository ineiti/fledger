use anyhow::Error;
use flarch::{
    nodeids::U256,
    tasks::{time::timeout, wait_ms},
};
use flcrypto::tofrombytes::ToFromBytes;
use flmodules::{
    dht_storage::{core::FloConfig, realm_view::RealmView},
    flo::{
        blob::{Blob, BlobAccess, BlobID, BlobPage},
        flo::{FloID, FloWrapper},
        realm::GlobalID,
    },
};
use std::str::FromStr;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use crate::simulation_dht_target::stats::SimulationStats;
use crate::{metrics::Metrics, Fledger};

#[derive(Clone)]
pub struct SimulationDhtTarget {}

impl SimulationDhtTarget {
    pub async fn run_create_fillers_and_target(
        mut f: Fledger,
        filler_amount: u32,
        page_size: u32,
        pages_propagation_delay: u32,
        connection_delay: u32,
        experiment_id: u32,
    ) -> anyhow::Result<()> {
        let metrics = Metrics::new(experiment_id, f.node.node_config.info.name.clone());

        f.loop_node(crate::FledgerState::DHTAvailable).await?;
        log::info!("DHT CONNECTED");

        log::info!("[Waiting for connections to settle]");
        log::info!("{} ms", connection_delay);
        wait_ms(connection_delay as u64).await;

        //let router = f.node.dht_router.unwrap();
        let mut rv = RealmView::new_first(f.node.dht_storage.clone().unwrap()).await?;

        log::info!("[Create filler pages]");
        for i in 0..filler_amount {
            let page_content = String::from_utf8(vec![b'-'; page_size as usize])?;
            let flo_page = rv
                .create_http(
                    &format!("simulation-filler-{}", i.to_string()),
                    page_content.clone(),
                    None,
                    flcrypto::access::Condition::Pass,
                    &[],
                )
                .await
                .unwrap();

            log::info!(
                "page {}/{}/{} | {} | {} ({}B -> {}B)",
                flo_page.flo_id(),
                flo_page.realm_id(),
                flo_page.version(),
                flo_page.values().iter().next().unwrap().1,
                page_content.clone().chars().take(50).collect::<String>(),
                page_content.size(),
                flo_page.size(),
            );
        }

        log::info!("[Waiting for fillers to settle]");
        log::info!("{} ms", pages_propagation_delay);

        // ds.broker.settle, ds.sync
        f.node
            .dht_storage
            .as_mut()
            .unwrap()
            .broker
            .settle(Vec::new())
            .await?;
        f.node.dht_storage.as_mut().unwrap().sync()?;
        wait_ms(pages_propagation_delay as u64).await;

        log::info!("[Sending simulation flo page]");
        let page_content = String::from_utf8(vec![b'o'; page_size as usize])?;
        let page_links: HashMap<String, Vec<BlobID>> = HashMap::new();
        let page_path = "simulation-page";
        let page_id = U256::zero();

        let flo_page = FloWrapper::from_type_config(
            rv.realm.realm_id(),
            flcrypto::access::Condition::Pass,
            FloConfig {
                cuckoo: flmodules::dht_storage::core::Cuckoo::None,
                force_id: Some(page_id),
            },
            BlobPage(Blob::make(
                "re.fledg.page".into(),
                page_links,
                [("path".to_string(), page_path.into())].into(),
                [("index.html".to_string(), page_content.into())].into(),
            )),
            &[],
        )?;
        // let flo_page = FloBlobPage::new_cuckoo(
        //     rv.realm.realm_id(),
        //     flcrypto::access::Condition::Pass,
        //     &format!("simulation-filler-{}", i.to_string()),
        //     Bytes::from(page_content),
        //     None,
        //     flmodules::dht_storage::core::Cuckoo::None.clone(),
        //     &[],
        // )?;
        // let flo_page = rv
        //     .create_http(
        //         "simulation-page",
        //         String::from_utf8(vec![b'o'; page_size as usize])?,
        //         None,
        //         flcrypto::access::Condition::Pass,
        //         &[],
        //     )
        //     .await
        //     .unwrap();

        let page_content =
            String::from_utf8(flo_page.datas().iter().next().unwrap().1.clone().to_vec()).unwrap();

        log::info!(
            "page {}/{}/{} | {} | {} ({}B -> {}B)",
            flo_page.flo_id(),
            flo_page.realm_id(),
            flo_page.version(),
            flo_page.values().iter().next().unwrap().1,
            page_content.chars().take(50).collect::<String>(),
            page_content.size(),
            flo_page.size(),
        );

        let signer = f.node.crypto_storage.get_signer();
        rv.set_realm_service("simulation-page", flo_page.blob_id(), &[&signer])
            .await?;

        f.node
            .dht_storage
            .as_mut()
            .unwrap()
            .store_flo(flo_page.flo().clone())?;
        wait_ms(1000).await;

        // ds.broker.settle, ds.sync
        f.node
            .dht_storage
            .as_mut()
            .unwrap()
            .broker
            .settle(Vec::new())
            .await?;
        f.node.dht_storage.as_mut().unwrap().sync()?;

        log::info!("SIMULATION END");
        metrics.success();

        f.loop_node(crate::FledgerState::Forever).await?;
        Ok(())
    }

    pub async fn fetch_target(
        mut f: Fledger,
        loop_delay: u32,
        enable_sync: bool,
        timeout_ms: u32,
        experiment_id: u32,
    ) -> anyhow::Result<()> {
        let start_instant = Instant::now();

        let metrics = Metrics::new(experiment_id, f.node.node_config.info.name.clone());
        let mut simulation_metrics = SimulationStats::new();

        let timeout_result = timeout(
            Duration::from_millis(timeout_ms.into()),
            f.loop_node(crate::FledgerState::DHTAvailable),
        )
        .await;

        if timeout_result.is_err() {
            log::warn!("SIMULATION TIMEOUT WHILE CONNECTING TO DHT");
            log::info!("SIMULATION END");
            metrics.timeout();
            return Err(timeout_result.unwrap_err().into());
        }

        log::info!("DHT CONNECTED");

        let realm_id = RealmView::new_first(f.node.dht_storage.as_ref().unwrap().clone())
            .await?
            .realm
            .realm_id();

        let mut iteration = 0u32;
        loop {
            if start_instant.elapsed().as_millis() > timeout_ms as u128 {
                log::warn!("SIMULATION TIMEOUT REACHED ({}ms)", timeout_ms);
                log::info!("SIMULATION END");
                metrics.timeout();
                f.loop_node(crate::FledgerState::Forever).await?;

                return Ok(());
            }

            wait_ms(loop_delay.into()).await;
            iteration += 1;

            if enable_sync {
                f.node.dht_storage.as_mut().unwrap().sync()?;
            }
            // let _ = rv
            //     .update_all()
            //     .await
            //     .inspect_err(|e| log::error!("error when doing rv.update_all(): {e}"));

            if iteration % 10 == 0 {
                simulation_metrics.refresh(&mut f).await;
                metrics.upload(simulation_metrics.clone());
            }

            let page_id = FloID::from_str(
                "c9e737fc7c55f404388d3eda20d5a047adc3b50e4ac59f25c0f9d8ce23d5fb94",
            )?;

            // todo!("test this");
            // continue; // Testing whether pages propagate with no
            // get_flo

            let page_global_id = GlobalID::new(realm_id.clone(), page_id.clone());
            let page_flo_wrapper_result: Result<FloWrapper<BlobPage>, Error> = f
                .node
                .dht_storage
                .as_mut()
                .unwrap()
                .get_flo(&page_global_id)
                .await;

            if let Ok(page_flo_wrapper) = page_flo_wrapper_result {
                let page_flo = page_flo_wrapper.flo();
                let page_blob =
                    BlobPage::from_rmp_bytes(page_flo.flo_type().as_str(), page_flo.data())?;
                let page_content = String::from_utf8(
                    page_blob
                        .0
                        .datas()
                        .iter()
                        .next()
                        .unwrap()
                        .1
                        .clone()
                        .to_vec(),
                )
                .unwrap_or_default();
                log::info!(
                    "simulation page found with content: {}",
                    page_content.chars().take(50).collect::<String>()
                );

                log::info!("SIMULATION END");
                simulation_metrics.refresh(&mut f).await;
                metrics.upload(simulation_metrics.clone());
                metrics.success();
                f.loop_node(crate::FledgerState::Forever).await?;

                return Ok(());
            }
        }
    }
}
