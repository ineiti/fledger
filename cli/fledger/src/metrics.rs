use std::{collections::HashMap, fs::File, time::Duration};

use crate::simulation_dht_target::stats::SimulationStats;
use anyhow::Error;
use metrics_exporter_influx::{InfluxBuilder, InfluxRecorderHandle};
use serde_json::Value;
use tokio::time::Instant;

#[derive(Clone)]
pub struct Metrics {
    experiment_id: u32,
    node_name: String,
    node_id: Option<String>,

    client: reqwest::blocking::Client,
}

impl Metrics {
    pub fn new(experiment_id: u32, node_name: String) -> Self {
        let mut metrics = Self {
            experiment_id,
            node_name,
            node_id: None,
            client: reqwest::blocking::Client::new(),
        };

        let _ = metrics.api_create_node();
        return metrics.clone();
    }

    pub fn setup(node_name: String) -> InfluxRecorderHandle {
        log::info!("Setting up metrics");
        let metrics_file = File::create(format!("/tmp/{}.metrics", node_name))
            .expect(format!("could not create /tmp/{}.metrics", node_name).as_ref());
        return InfluxBuilder::new()
            .with_duration(Duration::from_secs(10))
            .with_writer(metrics_file)
            .add_global_tag("node_name", node_name)
            .install()
            .expect("could not setup influx recorder");
    }

    pub fn upload(&self, stats: SimulationStats) {
        let mut data = HashMap::new();
        data.insert("pages", stats.pages.clone());
        self.api_put_data_node(data);

        let mut datapoints = HashMap::new();
        datapoints.insert(
            "connected_nodes_total",
            stats.connected_nodes_total.to_string(),
        );
        self.api_post_datapoints(datapoints);

        let mut timeless_datapoints = HashMap::new();
        timeless_datapoints.insert(
            "target_page_stored_bool",
            stats.target_page_stored_bool.to_string(),
        );
        timeless_datapoints.insert(
            "connected_nodes_total",
            stats.connected_nodes_total.to_string(),
        );
        timeless_datapoints.insert("pages_stored_total", stats.pages_stored_total.to_string());
        timeless_datapoints.insert("ds_size_bytes", stats.ds_size_bytes.to_string());

        timeless_datapoints.insert(
            "flos_sent_total",
            stats.ds_experiment_stats.max_flos_sent_in_flos.to_string(),
        );
        timeless_datapoints.insert(
            "available_flos_sent_total",
            stats.ds_experiment_stats.flo_value_sent_total.to_string(),
        );
        timeless_datapoints.insert(
            "available_flos_sent_blocked_total",
            stats
                .ds_experiment_stats
                .flo_value_sent_blocked_total
                .to_string(),
        );
        timeless_datapoints.insert(
            "max_flo_metas_received_in_available_flos",
            stats
                .ds_experiment_stats
                .max_flo_metas_received_in_available_flos
                .to_string(),
        );
        timeless_datapoints.insert(
            "max_flo_metas_requested_in_request_flos",
            stats
                .ds_experiment_stats
                .max_flo_metas_requested_in_request_flos
                .to_string(),
        );
        self.api_post_timeless_datapoints(timeless_datapoints);

        self.influx_push(stats)
    }

    pub fn upload_target_page_id(&self, target_page_id: String) {
        let mut data = HashMap::new();
        data.insert("target_page_id", target_page_id);
        self.api_put_data_experiment(data);
    }

    pub fn pull_page_id(&self) -> Option<String> {
        match self
            .client
            .get(format!(
                "https://fledger.yohan.ch/api/experiments/{}/target-page-id",
                self.experiment_id
            ))
            .header("Accept", "application/json")
            .header(
                "Authorization",
                "Bearer 1|d4EeHkRPlqwpgLpALyTor5FxHI4NWg1LXJtf5NZBfd82aa17",
            )
            .send()
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    let text = resp.text().unwrap();
                    log::info!("Successfully pulled page id: {}", text);
                    let response_data: Value = serde_json::from_str(&text).unwrap();
                    Some(
                        response_data["target_page_id"]
                            .as_str()
                            .unwrap()
                            .to_string(),
                    )
                } else {
                    log::error!("Failed to pull page id: {}", resp.status());
                    None
                }
            }
            Err(err) => {
                log::error!("Error pulling page id: {}", err);
                None
            }
        }
    }

    pub fn timeout(&self) {
        let mut data = HashMap::new();
        data.insert("status", "timeout".to_string());
        //absolute_counter!("fledger_simulation_timeout", 1);
        self.api_put_data_node(data);
    }

    pub fn success(&self) {
        let mut data = HashMap::new();
        data.insert("status", "success".to_string());
        //absolute_counter!("fledger_simulation_success", 1);
        self.api_put_data_node(data);
    }

    pub fn api_put_data_node(&self, data: HashMap<&str, String>) {
        self.api_put_data(
            format!(
                "https://fledger.yohan.ch/api/nodes/{}",
                self.node_id.clone().unwrap()
            ),
            data,
        );
    }

    pub fn api_put_data_experiment(&self, data: HashMap<&str, String>) {
        self.api_put_data(
            format!(
                "https://fledger.yohan.ch/api/experiments/{}",
                self.experiment_id
            ),
            data,
        );
    }

    pub fn api_put_data(&self, url: String, data: HashMap<&str, String>) {
        let start = Instant::now();
        match reqwest::blocking::Client::new()
            .put(url)
            .json(&data)
            .header("Accept", "application/json")
            .header(
                "Authorization",
                "Bearer 1|d4EeHkRPlqwpgLpALyTor5FxHI4NWg1LXJtf5NZBfd82aa17",
            )
            .send()
        {
            Ok(resp) => log::info!(
                "Successful API request in {}ms: {}",
                start.elapsed().as_millis(),
                resp.text().unwrap()
            ),
            Err(err) => log::error!("Error: {}", err),
        };
    }

    pub fn api_post_timeless_datapoints(&self, data: HashMap<&str, String>) {
        self.api_post_nodes("timeless-data-points".to_string(), data);
    }

    pub fn api_post_datapoints(&self, data: HashMap<&str, String>) {
        self.api_post_nodes("data-points".to_string(), data);
    }

    pub fn api_post_nodes(&self, resource: String, data: HashMap<&str, String>) {
        let start = Instant::now();
        match reqwest::blocking::Client::new()
            .post(format!(
                "https://fledger.yohan.ch/api/nodes/{}/{resource}",
                self.node_id.clone().unwrap()
            ))
            .json(&data)
            .header("Accept", "application/json")
            .header(
                "Authorization",
                "Bearer 1|d4EeHkRPlqwpgLpALyTor5FxHI4NWg1LXJtf5NZBfd82aa17",
            )
            .send()
        {
            Ok(resp) => log::info!(
                "Successful API request in {}ms ({resource}): {}",
                start.elapsed().as_millis(),
                resp.text().unwrap()
            ),
            Err(err) => log::error!("Error: {}", err),
        };
    }

    fn api_create_node(&mut self) -> Result<(), Error> {
        let mut data = HashMap::new();
        data.insert("name", self.node_name.clone());

        match self
            .client
            .post(format!(
                "https://fledger.yohan.ch/api/experiments/{}/nodes/",
                self.experiment_id
            ))
            .json(&data)
            .header("Accept", "application/json")
            .header(
                "Authorization",
                "Bearer 1|d4EeHkRPlqwpgLpALyTor5FxHI4NWg1LXJtf5NZBfd82aa17",
            )
            .send()
        {
            Ok(resp) => {
                let text = resp.text().unwrap();
                log::info!("Successfully created node in API: {}", text.clone());
                let response_data: Value = serde_json::from_str(&text.clone())?;
                self.node_id = Some(response_data["id"].to_string());
                return Ok(());
            }
            Err(err) => {
                log::error!("Error: {}", err);
                return Err(err.into());
            }
        };
    }

    fn create_influx_line(&self, measurement: String, value: u32) -> String {
        let node_name = self.node_name.clone();
        format!("{measurement},node_name={node_name} value={value}")
    }

    fn influx_push(&self, stats: SimulationStats) {
        let mut lines = Vec::new();
        lines.push(self.create_influx_line(
            "fledger_realm_storage_bytes".to_string(),
            stats.ds_size_bytes as u32,
        ));
        lines.push(
            self.create_influx_line("fledger_pages_total".to_string(), stats.pages_stored_total),
        );
        lines.push(self.create_influx_line(
            "fledger_simulation_page_stored".to_string(),
            stats.target_page_stored_bool as u32,
        ));
        lines.push(self.create_influx_line(
            "fledger_connected_total".to_string(),
            stats.connected_nodes_total,
        ));
        lines.push(self.create_influx_line(
            "fledger_forwarded_flo_requests_total".to_string(),
            stats.request_flos_received_total,
        ));
        lines.push(self.create_influx_line(
            "fledger_forwarded_flo_meta_requests_total".to_string(),
            stats.ds_experiment_stats.available_flos_sent_total,
        ));
        lines.push(self.create_influx_line(
            "fledger_blocked_flo_meta_requests_total".to_string(),
            stats.ds_experiment_stats.available_flos_sent_blocked_total,
        ));
        lines.push(
            self.create_influx_line(
                "fledger_flos_metas_received_from_neighbour".to_string(),
                stats
                    .ds_experiment_stats
                    .max_flo_metas_received_in_available_flos,
            ),
        );
        lines.push(
            self.create_influx_line(
                "fledger_flos_requested_from_neighbour".to_string(),
                stats
                    .ds_experiment_stats
                    .max_flo_metas_requested_in_request_flos,
            ),
        );
        lines.push(
            self.create_influx_line(
                "fledger_flos_ids_received_from_neighbour".to_string(),
                stats
                    .ds_experiment_stats
                    .max_flo_ids_received_in_request_flos,
            ),
        );
        lines.push(self.create_influx_line(
            "fledger_forwarded_flo_requests_total".to_string(),
            stats.ds_experiment_stats.flos_sent_total,
        ));
        lines.push(self.create_influx_line(
            "fledger_flos_sent_to_neighbour".to_string(),
            stats.ds_experiment_stats.max_flos_sent_in_flos,
        ));
        lines.push(self.create_influx_line(
            "fledger_blocked_flo_requests_total".to_string(),
            stats.ds_experiment_stats.flos_sent_blocked_total,
        ));
        lines.push(self.create_influx_line(
            "fledger_flo_value_sent_total".to_string(),
            stats.ds_experiment_stats.flo_value_sent_total,
        ));
        lines.push(self.create_influx_line(
            "fledger_flo_value_blocked_total".to_string(),
            stats.ds_experiment_stats.flo_value_sent_blocked_total,
        ));
        lines.push(self.create_influx_line(
            "fledger_blocked_flo_requests_total".to_string(),
            stats.ds_experiment_stats.flos_sent_blocked_total,
        ));
        lines.push(self.create_influx_line(
            "fledger_flos_received_from_neighbour".to_string(), // received
            stats.ds_experiment_stats.max_flos_received_in_flos,
        ));
        lines.push(self.create_influx_line(
            "fledger_ds_store_flo_total".to_string(),
            stats.ds_experiment_stats.store_flo_total,
        ));

        let influx_data = lines.join("\n");

        let start = Instant::now();
        match self.
            client
            .post("https://influxdb.abehssera.com/api/v2/write?org=fledger&bucket=fledger&precision=ns")
            .body(influx_data)
            .header(
                "Authorization",
                "Token F7y_RJHnXA0szQHDhEiuRDAw7B2etGywSc-wdMK-BJtkXwplqXe5ogCcXDEJJR18ZvWJ87kwxckl6n1lFu9B-Q==",
            )
            .send()
        {
            Ok(resp) => log::info!("Successful write to influxdb ({}ms): {}", start.elapsed().as_millis(),resp.text().unwrap()),
            Err(err) => log::error!("Error when writing to influxdb: {}", err),
        };
    }
}
