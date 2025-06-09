use crate::Fledger;
use flcrypto::tofrombytes::ToFromBytes;
use flmodules::dht_storage::broker::DHTStorage;
use flmodules::dht_storage::messages::ExperimentStats;
use flmodules::flo::blob::{BlobAccess, BlobPage};
use metrics::absolute_counter;
use std::any::type_name;

#[derive(Clone)]
pub struct SimulationStats {
    pub pages: String,
    pub target_page_stored_bool: u8,
    pub connected_nodes_total: u32,
    pub pages_stored_total: u32,
    pub ds_size_bytes: u64,

    pub request_flos_received_total: u32,
    pub ds_experiment_stats: ExperimentStats,
}

impl SimulationStats {
    pub fn new() -> Self {
        Self {
            pages: "".to_string(),
            target_page_stored_bool: 0,
            connected_nodes_total: 0,
            pages_stored_total: 0,
            ds_size_bytes: 0,

            request_flos_received_total: 0,
            ds_experiment_stats: ExperimentStats::new(),
        }
    }

    pub async fn refresh_pages(&mut self, ds: &mut DHTStorage) {
        let pages = ds
            .get_flos()
            .await
            .unwrap_or_else(|e| {
                log::error!("failed to get flos {e}");
                vec![]
            })
            .clone();
        let pages = pages
            .iter()
            .filter(|flo| flo.flo_type() == type_name::<BlobPage>())
            .map(|flo| BlobPage::from_rmp_bytes(&flo.flo_type(), &flo.data()).unwrap());

        let page_names = pages
            .clone()
            .map(|page| page.0.values().iter().next().unwrap().1.clone());

        let simulation_page_stored_this_iteration = page_names
            .clone()
            .find(|name| name == "simulation-page")
            .is_some();

        let page_list = page_names
            .clone()
            .map(|name| {
                return name.replace("simulation-filler-", "");
            })
            .collect::<Vec<String>>();

        let pages_csv = page_list.clone().join(", ");

        self.pages = pages_csv.clone();
        self.target_page_stored_bool = if simulation_page_stored_this_iteration {
            1
        } else {
            0
        };
        self.pages_stored_total = page_list.len() as u32;

        log::info!("pages stored: {pages_csv}");
    }

    pub async fn refresh(&mut self, f: &mut Fledger) {
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
        self.ds_experiment_stats = ds.stats.borrow().experiment_stats.clone();
        self.connected_nodes_total = connected_nodes_total;
        self.ds_size_bytes = ds_size;

        self.refresh_pages(ds).await;

        self.dump_influx();
    }

    pub fn dump_influx(&self) {
        absolute_counter!("fledger_realm_storage_bytes", self.ds_size_bytes);
        absolute_counter!("fledger_pages_total", self.pages_stored_total as u64);
        absolute_counter!(
            "fledger_simulation_page_stored",
            self.target_page_stored_bool as u64
        );
        absolute_counter!("fledger_connected_total", self.connected_nodes_total as u64);
        absolute_counter!(
            "fledger_forwarded_flo_requests_total",
            self.request_flos_received_total as u64
        );
        absolute_counter!(
            "fledger_forwarded_flo_meta_requests_total",
            self.ds_experiment_stats.available_flos_sent_total as u64
        );
        absolute_counter!(
            "fledger_blocked_flo_meta_requests_total",
            self.ds_experiment_stats.available_flos_sent_blocked_total as u64
        );
        absolute_counter!(
            "fledger_flos_metas_received_from_neighbour",
            self.ds_experiment_stats
                .max_flo_metas_received_in_available_flos as u64
        );
        absolute_counter!(
            "fledger_flos_requested_from_neighbour",
            self.ds_experiment_stats
                .max_flo_metas_requested_in_request_flos as u64
        );
        absolute_counter!(
            "fledger_flos_ids_received_from_neighbour",
            self.ds_experiment_stats
                .max_flo_ids_received_in_request_flos as u64
        );
        absolute_counter!(
            "fledger_forwarded_flo_requests_total",
            self.ds_experiment_stats.flos_sent_total as u64
        );
        absolute_counter!(
            "fledger_flos_sent_to_neighbour",
            self.ds_experiment_stats.max_flos_sent_in_flos as u64
        );
        absolute_counter!(
            "fledger_blocked_flo_requests_total",
            self.ds_experiment_stats.flos_sent_blocked_total as u64
        );
        absolute_counter!(
            "fledger_flo_value_sent_total",
            self.ds_experiment_stats.flo_value_sent_total as u64
        );
        absolute_counter!(
            "fledger_flo_value_blocked_total",
            self.ds_experiment_stats.flo_value_sent_blocked_total as u64
        );
        absolute_counter!(
            "fledger_blocked_flo_requests_total",
            self.ds_experiment_stats.flos_sent_blocked_total as u64
        );
        absolute_counter!(
            "fledger_flos_received_from_neighbour", // received
            self.ds_experiment_stats.max_flos_received_in_flos as u64
        );
        absolute_counter!(
            "fledger_ds_store_flo_total",
            self.ds_experiment_stats.store_flo_total as u64
        );
    }
}
