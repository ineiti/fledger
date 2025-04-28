use std::{fs::File, time::Duration};

use metrics_exporter_influx::{InfluxBuilder, InfluxRecorderHandle};

pub struct Metrics {}

impl Metrics {
    pub fn setup(node_name: String) -> InfluxRecorderHandle {
        log::info!("Setting up metrics");
        let metrics_file = File::create(format!("/tmp/{}.metrics", node_name))
            .expect(format!("could not create /tmp/{}.metrics", node_name).as_ref());
        return InfluxBuilder::new()
            .with_duration(Duration::from_secs(1))
            .with_writer(metrics_file)
            .add_global_tag("node_name", node_name)
            .install()
            .expect("could not setup influx recorder");
    }
}
