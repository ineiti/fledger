use crate::hermes::snapshot::Snapshot;
use crate::hermes::update_response::UpdateResponse;
use crate::state::{Page, SimulationState};
use anyhow::Error;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tokio::time::Instant;

// Command and Control server for the Fledger CLI
//
// Hermes is both a command and control server and a metrics collector for the Fledger CLI.
// It has a dashboard that allows users to monitor the state of their Fledger nodes,
// view logs, and control the nodes remotely.
#[derive(Default, Clone, Debug)]
pub struct HermesApi {
    client: reqwest::blocking::Client,
    node_id: u32,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct LostTargetPagesResponse {
    pub lost_target_pages: Vec<String>,
}

impl HermesApi {
    pub fn update(&self, state: SimulationState) -> Result<UpdateResponse, Error> {
        let snapshot = Snapshot::make(state.clone());
        let url = format!("https://fledger.yohan.ch/api/nodes/{}", state.node_id);
        let response = self.api_request(Method::PUT, url, &snapshot)?;
        let bot_state: UpdateResponse = serde_json::from_str(&response)?;
        Ok(bot_state)
    }

    pub fn create_node(&mut self, experiment_id: u32, node_name: String) -> u32 {
        let mut data = HashMap::new();
        data.insert("name", node_name.clone());
        let url = format!(
            "https://fledger.yohan.ch/api/experiments/{}/nodes",
            experiment_id
        );

        let text = self.api_request(Method::POST, url, &data).unwrap();
        let json: Value = serde_json::from_str(&text).unwrap();
        let id_opt = json["id"].as_u64();
        let id = if id_opt.is_none() {
            panic!("Failed to create node: no ID returned");
        } else {
            id_opt.unwrap() as u32
        };
        self.node_id = id;
        id
    }

    pub fn get_lost_target_pages(
        &self,
        experiment_id: u32,
    ) -> Result<LostTargetPagesResponse, Error> {
        let url = format!(
            "https://fledger.yohan.ch/api/experiments/{}/lost-target-pages",
            experiment_id,
        );
        let response = self.api_request(Method::GET, url, &())?;
        let lost_target_pages: LostTargetPagesResponse =
            serde_json::from_str(&response).map_err(|e| Error::new(e))?;
        Ok(lost_target_pages)
    }

    pub fn start_fetching(&self, experiment_id: u32) -> Result<(), Error> {
        let url = format!(
            "https://fledger.yohan.ch/api/experiments/{}/start-fetching",
            experiment_id
        );
        self.api_request(Method::GET, url, &())?;
        Ok(())
    }

    pub fn store_target_pages(
        &self,
        experiment_id: u32,
        target_pages: Vec<Page>,
    ) -> Result<(), Error> {
        let mut data = HashMap::new();
        data.insert("target_pages", target_pages);
        let url = format!(
            "https://fledger.yohan.ch/api/experiments/{}/store-target-pages",
            experiment_id
        );
        self.api_request(Method::POST, url, &data)?;
        Ok(())
    }

    pub fn set_node_target_pages(&self, target_pages: Vec<Page>) -> Result<UpdateResponse, Error> {
        let mut data = HashMap::new();
        let ids = target_pages
            .iter()
            .map(|page| page.id.clone())
            .collect::<Vec<String>>();
        data.insert("stored_targets", ids);
        let url = format!(
            "https://fledger.yohan.ch/api/nodes/{}/set-target-pages",
            self.node_id.clone(),
        );
        let response = self.api_request(Method::POST, url, &data)?;
        let target_to_fetch: UpdateResponse = serde_json::from_str(&response)?;
        Ok(target_to_fetch)
    }

    fn api_request<T: Serialize + ?Sized>(
        &self,
        method: Method,
        url: String,
        data: &T,
    ) -> Result<String, Error> {
        let start = Instant::now();
        match self
            .client
            .request(method, url)
            .json(data)
            .header("Accept", "application/json")
            .header(
                "Authorization",
                "Bearer 1|d4EeHkRPlqwpgLpALyTor5FxHI4NWg1LXJtf5NZBfd82aa17",
            )
            .send()
        {
            Ok(resp) => {
                let text = resp.text()?;
                log::info!(
                    "Successful API request in {}ms: {}",
                    start.elapsed().as_millis(),
                    text.clone()
                );
                Ok(text)
            }
            Err(err) => {
                log::error!("Error: {}", err);
                Err(Error::new(err))
            }
        }
    }
}
