use crate::state::SimulationState;

// Influx Line Protocol for Fledger
pub struct InfluxLines {
    node_name: String,
}

impl InfluxLines {
    fn create_influx_line(&self, measurement: String, value: u32) -> String {
        format!("{measurement},node_name={} value={value}", self.node_name)
    }

    pub fn make_influx_data(node_name: String, stats: SimulationState) -> String {
        let generator = Self { node_name };

        let mut lines = Vec::new();
        lines.push(generator.create_influx_line(
            "fledger_realm_storage_bytes".to_string(),
            stats.ds_size_bytes as u32,
        ));
        lines.push(generator.create_influx_line(
            "fledger_pages_total".to_string(),
            stats.pages_stored.len() as u32,
        ));
        lines.push(generator.create_influx_line(
            "fledger_connected_total".to_string(),
            stats.connected_nodes_total,
        ));
        // lines.push(generator.create_influx_line(
        //     "fledger_forwarded_flo_requests_total".to_string(),
        //     stats.request_flos_received_total,
        // ));
        lines.push(generator.create_influx_line(
            "fledger_forwarded_flo_meta_requests_total".to_string(),
            stats.ds_metrics.available_flos_sent_total,
        ));
        lines.push(generator.create_influx_line(
            "fledger_blocked_flo_meta_requests_total".to_string(),
            stats.ds_metrics.available_flos_sent_blocked_total,
        ));
        lines.push(generator.create_influx_line(
            "fledger_flos_metas_received_from_neighbour".to_string(),
            stats.ds_metrics.max_flo_metas_received_in_available_flos,
        ));
        lines.push(generator.create_influx_line(
            "fledger_flos_requested_from_neighbour".to_string(),
            stats.ds_metrics.max_flo_metas_requested_in_request_flos,
        ));
        lines.push(generator.create_influx_line(
            "fledger_flos_ids_received_from_neighbour".to_string(),
            stats.ds_metrics.max_flo_ids_received_in_request_flos,
        ));
        lines.push(generator.create_influx_line(
            "fledger_forwarded_flo_requests_total".to_string(),
            stats.ds_metrics.flos_sent_total,
        ));
        lines.push(generator.create_influx_line(
            "fledger_flos_sent_to_neighbour".to_string(),
            stats.ds_metrics.max_flos_sent_in_flos,
        ));
        lines.push(generator.create_influx_line(
            "fledger_blocked_flo_requests_total".to_string(),
            stats.ds_metrics.flos_sent_blocked_total,
        ));
        lines.push(generator.create_influx_line(
            "fledger_flo_value_sent_total".to_string(),
            stats.ds_metrics.flo_value_sent_total,
        ));
        lines.push(generator.create_influx_line(
            "fledger_flo_value_blocked_total".to_string(),
            stats.ds_metrics.flo_value_sent_blocked_total,
        ));
        lines.push(generator.create_influx_line(
            "fledger_blocked_flo_requests_total".to_string(),
            stats.ds_metrics.flos_sent_blocked_total,
        ));
        lines.push(generator.create_influx_line(
            "fledger_flos_received_from_neighbour".to_string(), // received
            stats.ds_metrics.max_flos_received_in_flos,
        ));
        lines.push(generator.create_influx_line(
            "fledger_ds_store_flo_total".to_string(),
            stats.ds_metrics.store_flo_total,
        ));

        if let Some(target_page_stored_bool) = stats.target_page_stored_bool {
            lines.push(generator.create_influx_line(
                "fledger_simulation_dht_target_state".to_string(),
                target_page_stored_bool as u32,
            ));
        }

        lines.join("\n")
    }
}
