use std::{collections::HashMap, fs::File, time::Duration};

use anyhow::Error;
use flmodules::dht_storage::messages::ExperimentStats;
use metrics_exporter_influx::{InfluxBuilder, InfluxRecorderHandle};
use serde_json::Value;

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

    pub fn update(&self, pages_csv: String, experiment_stats: ExperimentStats) {
        let mut data = HashMap::new();
        data.insert("pages", pages_csv.clone());
        data.insert(
            "amount_flo_value_sent",
            experiment_stats.amount_flo_value_sent().to_string(),
        );
        data.insert(
            "amount_request_flo_metas_received",
            experiment_stats
                .amount_request_flo_metas_received()
                .to_string(),
        );
        self.api_put_data(data);

        let mut datapoints = HashMap::new();
        datapoints.insert(
            "amount_request_flo_metas_received",
            experiment_stats
                .amount_request_flo_metas_received()
                .to_string(),
        );
        self.api_post_datapoints(datapoints);

        let mut timeless_datapoints = HashMap::new();
        timeless_datapoints.insert(
            "amount_flo_value_sent",
            experiment_stats.amount_flo_value_sent().to_string(),
        );
        timeless_datapoints.insert(
            "amount_request_flo_metas_received",
            experiment_stats
                .amount_request_flo_metas_received()
                .to_string(),
        );
        self.api_post_timeless_datapoints(timeless_datapoints);
    }

    pub fn timeout(&self) {
        let mut data = HashMap::new();
        data.insert("status", "timeout".to_string());
        self.api_put_data(data);
    }

    pub fn success(&self) {
        let mut data = HashMap::new();
        data.insert("status", "success".to_string());
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
