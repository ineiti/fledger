use anyhow::Error;
use flarch::tasks::{time::timeout, wait_ms};
use flcrypto::tofrombytes::ToFromBytes;
use flmodules::{
    dht_storage::realm_view::RealmView,
    flo::{
        blob::{BlobAccess, BlobPage},
        flo::{FloID, FloWrapper},
        realm::GlobalID,
    },
};
use std::str::FromStr;
use std::time::{Duration, Instant};

use crate::state::SimulationState;
use crate::Fledger;

#[derive(Clone)]
pub struct SimulationDhtTarget {}

impl SimulationDhtTarget {
    fn log_page_info(flo_page: &FloWrapper<BlobPage>) {
        let page_content =
            String::from_utf8(flo_page.datas().iter().next().unwrap().1.clone().to_vec())
                .unwrap_or_default();
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
    }

    async fn create_flo_page(
        rv: &mut RealmView,
        name: &str,
        content: String,
    ) -> anyhow::Result<FloWrapper<BlobPage>> {
        let flo_page = rv
            .create_http(name, content, None, flcrypto::access::Condition::Pass, &[])
            .await?;
        Self::log_page_info(&flo_page);
        Ok(flo_page)
    }

    async fn settle_and_sync(f: &mut Fledger) -> anyhow::Result<()> {
        // ds.settle, ds.sync
        f.node
            .dht_storage
            .as_mut()
            .unwrap()
            .broker
            .settle(Vec::new())
            .await?;
        f.node.dht_storage.as_mut().unwrap().sync()?;
        Ok(())
    }

    pub async fn run_create_fillers_and_target(
        mut f: Fledger,
        filler_amount: u32,
        page_size: u32,
        pages_propagation_delay: u32,
        connection_delay: u32,
        experiment_id: u32,
    ) -> anyhow::Result<()> {
        let mut state = SimulationState::new(experiment_id, f.node.node_config.info.name.clone());

        f.loop_node(crate::FledgerState::DHTAvailable).await?;
        log::info!("DHT CONNECTED");

        log::info!("[Waiting for connections to settle]");
        log::info!("{} ms", connection_delay);
        wait_ms(connection_delay as u64).await;

        //let router = f.node.dht_router.unwrap();
        let mut rv = RealmView::new_first(f.node.dht_storage.clone().unwrap()).await?;

        log::info!("[Create filler pages]");
        for i in 0..filler_amount {
            wait_ms(500).await;
            Self::create_flo_page(
                &mut rv,
                &format!("simulation-filler-{}", i.to_string()),
                String::from_utf8(vec![b'-'; page_size as usize])?,
            )
            .await?;
        }

        log::info!("[Waiting for fillers to settle]");
        log::info!("{} ms", pages_propagation_delay);

        Self::settle_and_sync(&mut f).await?;
        wait_ms(pages_propagation_delay as u64).await;

        log::info!("[Sending simulation flo page]");
        let flo_page = Self::create_flo_page(
            &mut rv,
            "simulation-page",
            String::from_utf8(vec![b'o'; page_size as usize])?,
        )
        .await?;

        log::info!("[Waiting for target page to propagate]");
        log::info!("5000 ms");
        wait_ms(5000).await;

        state.target_page_id = Some(flo_page.flo_id().to_string());
        state.update_and_upload(&mut f).await;

        wait_ms(1000).await;

        Self::settle_and_sync(&mut f).await?;

        log::info!("SIMULATION END");
        state.success();
        state.update_and_upload(&mut f).await;

        f.loop_node(crate::FledgerState::Forever).await?;
        Ok(())
    }

    pub async fn fetch_target(
        mut f: Fledger,
        loop_delay: u32,
        enable_sync: bool,
        timeout_ms: u32,
        experiment_id: u32,
        evil_noforward: bool,
    ) -> anyhow::Result<()> {
        let start_instant = Instant::now();

        let node_name = f.node.node_config.info.name.clone();
        let mut state = SimulationState::new(experiment_id, node_name);

        let timeout_result = timeout(
            Duration::from_millis(timeout_ms.into()),
            f.loop_node(crate::FledgerState::DHTAvailable),
        )
        .await;

        if timeout_result.is_err() {
            log::warn!("SIMULATION TIMEOUT WHILE CONNECTING TO DHT");
            log::info!("SIMULATION END");
            state.timeout();
            return Err(timeout_result.unwrap_err().into());
        }

        log::info!("DHT CONNECTED");

        let realm_id = RealmView::new_first(f.node.dht_storage.as_ref().unwrap().clone())
            .await?
            .realm
            .realm_id();

        let mut iteration = 0u32;
        let mut page_id_opt: Option<String> = None;

        // Loop until page_id found

        loop {
            if start_instant.elapsed().as_millis() > timeout_ms as u128 {
                log::warn!("SIMULATION TIMEOUT REACHED ({}ms)", timeout_ms);
                log::info!("SIMULATION END");
                state.timeout();
                state.update_and_upload(&mut f).await;
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
                let response = state.update_and_upload(&mut f).await;
                page_id_opt = response.target_page_id;

                if page_id_opt.is_some() {
                    // this node shall become malicious now if the flag is set
                    // because the target page was propagated
                    log::info!("(re)becoming malicious node");
                    unsafe {
                        flmodules::dht_storage::messages::EVIL_NO_FORWARD = evil_noforward;
                        flmodules::dht_router::messages::EVIL_NO_FORWARD = evil_noforward;
                    }
                }
            }

            // Stop here each iteration
            // until page id is known
            if page_id_opt.is_none() {
                log::info!("failed to get page id");
                continue;
            }

            let page_id = FloID::from_str(&page_id_opt.clone().unwrap())?;

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
                state.success();
                state.update_and_upload(&mut f).await;
                f.loop_node(crate::FledgerState::Forever).await?;

                return Ok(());
            }
        }
    }
}
