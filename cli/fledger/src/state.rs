use crate::hermes::api::HermesApi;
use crate::hermes::update_response::UpdateResponse;
use crate::influx::api::InfluxApi;
use crate::influx::lines::InfluxLines;
use crate::Fledger;
use flcrypto::tofrombytes::ToFromBytes;
use flmodules::dht_storage::broker::DHTStorage;
use flmodules::dht_storage::messages::DsMetrics;
use flmodules::flo::blob::{BlobAccess, BlobPage};
use serde::{Deserialize, Serialize};
use std::any::type_name;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Page {
    pub name: String,
    pub id: String,
}

#[derive(Clone, Debug, Default)]
pub struct SimulationState {
    pub experiment_id: u32,
    pub node_id: u32,
    pub node_name: String,
    pub node_status: String,

    pub pages_stored: Vec<Page>,
    pub connected_nodes_total: u32,
    pub ds_size_bytes: u64,
    pub evil_no_forward: bool,
    pub target_successfully_fetched_total: u32,
    pub fetch_requests_total: u32,

    pub ds_metrics: DsMetrics,

    pub api: HermesApi,
    pub influx: InfluxApi,
}

impl SimulationState {
    pub fn new(experiment_id: u32, node_name: String) -> Self {
        let mut state = Self::default();

        let node_id = state.api.create_node(experiment_id, node_name.clone());

        state.experiment_id = experiment_id;
        state.node_id = node_id;
        state.node_name = node_name;
        state.node_status = "active".to_string();
        state
    }

    pub fn increment_fetch_requests_total(&mut self) {
        self.fetch_requests_total += 1;
    }

    pub async fn refresh_pages(&mut self, ds: &mut DHTStorage) {
        let pages_stored = ds
            .get_flos()
            .await
            .unwrap_or_else(|e| {
                log::error!("failed to get flos {e}");
                vec![]
            })
            .iter()
            .filter(|flo| flo.flo_type() == type_name::<BlobPage>())
            .map(|flo| {
                let page = BlobPage::from_rmp_bytes(&flo.flo_type(), &flo.data()).unwrap();
                Page {
                    id: flo.flo_id().to_string(),
                    name: page.0.values().iter().next().unwrap().1.clone(),
                }
            })
            .collect::<Vec<Page>>();

        self.pages_stored = pages_stored.clone();
        log::info!(
            "pages stored: {}",
            pages_stored
                .iter()
                .map(|page| page.name.clone())
                .collect::<Vec<String>>()
                .join(", ")
        );
    }

    pub async fn get_stored_targets(&mut self, ds: &mut DHTStorage) -> Vec<Page> {
        ds.get_flos()
            .await
            .unwrap_or_else(|e| {
                log::error!("failed to get flos {e}");
                vec![]
            })
            .iter()
            .filter(|flo| flo.flo_type() == type_name::<BlobPage>())
            .map(|flo| {
                let page = BlobPage::from_rmp_bytes(&flo.flo_type(), &flo.data()).unwrap();
                Page {
                    id: flo.flo_id().to_string(),
                    name: page.0.values().iter().next().unwrap().1.clone(),
                }
            })
            .filter(|page| page.name.starts_with("target"))
            .collect::<Vec<Page>>()
    }

    pub fn success(&mut self) {
        self.node_status = "success".to_string();
    }

    pub fn timeout(&mut self) {
        self.node_status = "timeout".to_string();
    }

    pub async fn update_and_upload(&mut self, f: &mut Fledger) -> UpdateResponse {
        let connected_nodes_total = f
            .node
            .dht_router
            .clone()
            .unwrap()
            .stats
            .borrow()
            .active
            .clone() as u32;
        let ds = f.node.dht_storage.as_mut().unwrap();
        let ds_size = ds.stats.borrow().realm_stats.iter().next().unwrap().1.size;
        self.ds_metrics = ds.stats.borrow().experiment_stats.clone();
        self.connected_nodes_total = connected_nodes_total;
        self.ds_size_bytes = ds_size;
        self.evil_no_forward = f.args.evil_noforward.clone();
        self.refresh_pages(ds).await;

        self.upload()
    }

    pub async fn send_target_pages(&mut self, f: &mut Fledger) -> UpdateResponse {
        let target_pages = self
            .get_stored_targets(f.node.dht_storage.as_mut().unwrap())
            .await;
        self.api
            .set_node_target_pages(target_pages)
            .unwrap_or_else(|e| {
                log::error!("failed to set target pages: {e}");
                UpdateResponse::default()
            })
    }

    pub fn upload(&mut self) -> UpdateResponse {
        let node_name = self.node_name.clone();
        let _ = self
            .influx
            .write(InfluxLines::make_influx_data(node_name, self.clone()));

        self.api.update(self.clone()).unwrap_or_else(|e| {
            log::error!("failed to update simulation state: {e}");
            UpdateResponse::default()
        })
    }
}
