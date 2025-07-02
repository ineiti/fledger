use crate::hermes::api::HermesApi;
use crate::state::{Page, SimulationState};
use crate::Fledger;
use flarch::tasks::{time::timeout, wait_ms};
use flcrypto::tofrombytes::ToFromBytes;
use flmodules::flo::realm::RealmID;
use flmodules::{
    dht_storage::realm_view::RealmView,
    flo::{
        blob::{BlobAccess, BlobPage},
        flo::{FloID, FloWrapper},
        realm::GlobalID,
    },
};
use std::collections::HashSet;
use std::str::FromStr;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct SimulationDht {}

impl SimulationDht {
    fn get_page_content(page: &FloWrapper<BlobPage>) -> String {
        String::from_utf8(page.datas().iter().next().unwrap().1.clone().to_vec()).unwrap()
    }

    fn get_page_name(page: &FloWrapper<BlobPage>) -> String {
        page.values().iter().next().unwrap().1.clone()
    }

    fn log_page_info(flo_page: &FloWrapper<BlobPage>) {
        let page_content = Self::get_page_content(flo_page);
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

    fn make_page_id(realm_id: RealmID, page_id: FloID) -> GlobalID {
        GlobalID::new(realm_id.clone(), page_id.clone())
    }

    async fn fetch_page(f: &mut Fledger, id: GlobalID) -> anyhow::Result<FloWrapper<BlobPage>> {
        f.node.dht_storage.as_mut().unwrap().get_flo(&id).await
    }

    pub async fn run_create_pages(
        mut f: Fledger,
        filler_amount: u32,
        target_amount: u32,
        page_size: u32,
        propagation_delay: u32,
        connection_delay: u32,
        experiment_id: u32,
    ) -> anyhow::Result<()> {
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
                &format!("filler-{}", i.to_string()),
                String::from_utf8(vec![b'-'; page_size as usize])?,
            )
            .await?;
        }

        log::info!("[Create target_pages simulation flo page]");
        let mut target_pages = vec![];
        for i in 0..target_amount {
            wait_ms(500).await;
            target_pages.push(
                Self::create_flo_page(
                    &mut rv,
                    &format!("target-{}", i.to_string()),
                    String::from_utf8(vec![b'o'; page_size as usize])?,
                )
                .await?,
            );
        }

        log::info!("[Waiting for pages to settle]");
        log::info!("{} ms", propagation_delay);
        Self::settle_and_sync(&mut f).await?;
        wait_ms(propagation_delay as u64).await;

        let pages = target_pages
            .iter()
            .map(|page| Page {
                id: page.flo_id().to_string(),
                name: Self::get_page_name(page),
            })
            .collect::<Vec<Page>>();

        HermesApi::default().store_target_pages(experiment_id, pages)?;

        log::info!("SIMULATION END");

        f.loop_node(crate::FledgerState::Forever).await?;
        Ok(())
    }

    pub async fn run_fetch_pages(
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

        let mut target_page_ids = HashSet::new();
        let mut fetched_page_ids = HashSet::new();

        let mut iteration = 0u32;

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

            if iteration % 30 == 0 {
                let response = state.update_and_upload(&mut f).await;
                target_page_ids.clear();
                for target_page_id in response.target_page_ids {
                    target_page_ids.insert(target_page_id.clone());
                }

                if !target_page_ids.is_empty() && evil_noforward {
                    unsafe {
                        if evil_noforward && !flmodules::dht_storage::messages::EVIL_NO_FORWARD {
                            log::info!("becoming a malicious node");
                            log::info!("SIMULATION END");
                            flmodules::dht_storage::messages::EVIL_NO_FORWARD = true;
                            flmodules::dht_router::messages::EVIL_NO_FORWARD = true;
                        }
                    }
                }
            }

            for page_id in target_page_ids.clone() {
                if fetched_page_ids.contains(&page_id.clone()) {
                    continue;
                }
                let flo_id = FloID::from_str(&page_id.clone())?;
                let global_page_id = Self::make_page_id(realm_id.clone(), flo_id);
                let page = Self::fetch_page(&mut f, global_page_id).await;
                if page.is_ok() {
                    fetched_page_ids.insert(page_id.clone());
                }
            }

            // log fetched and target pages
            log::info!("fetched pages: {fetched_page_ids:?}");
            log::info!("target pages: {target_page_ids:?}");

            state.target_successfully_fetched_total = fetched_page_ids.len() as u32;

            if !target_page_ids.is_empty() && fetched_page_ids.is_superset(&target_page_ids) {
                log::info!("all target pages fetched.");
                log::info!("SIMULATION END");
                state.success();
                state.update_and_upload(&mut f).await;
                f.loop_node(crate::FledgerState::Forever).await?;
            }
        }
    }
}
