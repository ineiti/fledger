use std::{collections::HashMap, fs::File, time::Duration};

use anyhow::Error;
use metrics_exporter_influx::{InfluxBuilder, InfluxRecorderHandle};
use serde_json::Value;

use crate::simulation_dht_target::stats::SimulationStats;

#[derive(Clone)]
pub struct Metrics {
    experiment_id: u32,
    node_name: String,
    node_id: Option<String>,
}

impl Metrics {
    pub fn new(experiment_id: u32, node_name: String) -> Self {
        let mut metrics = Self {
            experiment_id,
            node_name,
            node_id: None,
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

    pub fn upload(&self, simulation_metrics: SimulationStats) {
        let mut data = HashMap::new();
        data.insert("pages", simulation_metrics.pages.clone());
        self.api_put_data(data);

        let mut datapoints = HashMap::new();
        datapoints.insert(
            "connected_nodes_total",
            simulation_metrics.connected_nodes_total.to_string(),
        );
        self.api_post_datapoints(datapoints);

        let mut timeless_datapoints = HashMap::new();
        timeless_datapoints.insert(
            "target_page_stored_bool",
            simulation_metrics.target_page_stored_bool.to_string(),
        );
        timeless_datapoints.insert(
            "connected_nodes_total",
            simulation_metrics.connected_nodes_total.to_string(),
        );
        timeless_datapoints.insert(
            "pages_stored_total",
            simulation_metrics.pages_stored_total.to_string(),
        );
        timeless_datapoints.insert(
            "ds_size_bytes",
            simulation_metrics.ds_size_bytes.to_string(),
        );

        timeless_datapoints.insert(
            "flos_sent_total",
            simulation_metrics
                .ds_experiment_stats
                .max_flos_sent_in_flos
                .to_string(),
        );
        timeless_datapoints.insert(
            "available_flos_sent_total",
            simulation_metrics
                .ds_experiment_stats
                .flo_value_sent_total
                .to_string(),
        );
        timeless_datapoints.insert(
            "available_flos_sent_blocked_total",
            simulation_metrics
                .ds_experiment_stats
                .flo_value_sent_blocked_total
                .to_string(),
        );
        timeless_datapoints.insert(
            "max_flo_metas_received_in_available_flos",
            simulation_metrics
                .ds_experiment_stats
                .max_flo_metas_received_in_available_flos
                .to_string(),
        );
        timeless_datapoints.insert(
            "max_flo_metas_requested_in_request_flos",
            simulation_metrics
                .ds_experiment_stats
                .max_flo_metas_requested_in_request_flos
                .to_string(),
        );
        self.api_post_timeless_datapoints(timeless_datapoints);
    }

    pub fn timeout(&self) {
        let mut data = HashMap::new();
        data.insert("status", "timeout".to_string());
        //absolute_counter!("fledger_simulation_timeout", 1);
        self.api_put_data(data);
    }

    pub fn success(&self) {
        let mut data = HashMap::new();
        data.insert("status", "success".to_string());
        //absolute_counter!("fledger_simulation_success", 1);
        self.api_put_data(data);
    }

    pub fn api_put_data(&self, data: HashMap<&str, String>) {
        match reqwest::blocking::Client::new()
            .put(format!(
                "https://fledger.yohan.ch/api/nodes/{}",
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
            Ok(resp) => log::info!("Successful API request: {}", resp.text().unwrap()),
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
                "Successful API request ({resource}): {}",
                resp.text().unwrap()
            ),
            Err(err) => log::error!("Error: {}", err),
        };
    }

    fn api_create_node(&mut self) -> Result<(), Error> {
        let mut data = HashMap::new();
        data.insert("name", self.node_name.clone());

        match reqwest::blocking::Client::new()
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
}
