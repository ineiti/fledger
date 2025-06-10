use crate::state::SimulationState;
use serde::Serialize;

#[derive(Serialize, Default, Debug, Clone)]
pub struct Snapshot {
    node_status: String,
    pages_stored: Vec<String>,
    evil_no_forward: bool,
    target_page_id: Option<String>,

    timed_metrics: Vec<(String, u32)>,
    timeless_metrics: Vec<(String, u32)>,
}

struct MetricsBuilder {
    metrics: Vec<(String, u32)>,
}

impl MetricsBuilder {
    pub fn new() -> Self {
        MetricsBuilder {
            metrics: Vec::new(),
        }
    }

    pub fn add_metric(&mut self, key: String, value: u32) {
        self.metrics.push((key, value));
    }

    pub fn build(self) -> Vec<(String, u32)> {
        self.metrics.clone()
    }
}

impl Snapshot {
    pub fn make(simulation_state: SimulationState) -> Self {
        let mut timed_metrics = MetricsBuilder::new();
        timed_metrics.add_metric(
            "pages_stored".to_owned(),
            simulation_state.pages_stored.len() as u32,
        );

        let mut timeless_metrics = MetricsBuilder::new();
        timeless_metrics.add_metric(
            "connected_nodes_total".to_string(),
            simulation_state.connected_nodes_total,
        );
        timeless_metrics.add_metric(
            "pages_stored_total".to_string(),
            simulation_state.pages_stored.len() as u32,
        );
        timeless_metrics.add_metric(
            "ds_size_bytes".to_string(),
            simulation_state.ds_size_bytes as u32,
        );
        timeless_metrics.add_metric(
            "flos_sent_total".to_string(),
            simulation_state.ds_metrics.max_flos_sent_in_flos,
        );
        timeless_metrics.add_metric(
            "available_flos_sent_total".to_string(),
            simulation_state.ds_metrics.flo_value_sent_total,
        );
        timeless_metrics.add_metric(
            "available_flos_sent_blocked_total".to_string(),
            simulation_state.ds_metrics.flo_value_sent_blocked_total,
        );
        timeless_metrics.add_metric(
            "max_flo_metas_received_in_available_flos".to_string(),
            simulation_state
                .ds_metrics
                .max_flo_metas_received_in_available_flos,
        );
        timeless_metrics.add_metric(
            "max_flo_metas_requested_in_request_flos".to_string(),
            simulation_state
                .ds_metrics
                .max_flo_metas_requested_in_request_flos,
        );

        if let Some(target_page_stored_bool) = simulation_state.target_page_stored_bool {
            timeless_metrics.add_metric(
                "target_page_stored_bool".to_string(),
                target_page_stored_bool as u32,
            );
        }

        Snapshot {
            node_status: simulation_state.node_status,
            pages_stored: simulation_state.pages_stored,
            evil_no_forward: simulation_state.evil_no_forward,
            target_page_id: simulation_state.target_page_id,

            timed_metrics: timed_metrics.build(),
            timeless_metrics: timeless_metrics.build(),
        }
    }
}
