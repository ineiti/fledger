pub mod config;
pub mod core;
pub mod sphinx;
pub mod storage;

pub mod client;
pub mod mixnode;
pub mod provider;

pub mod messages;

pub mod broker;

use prometheus::{register_counter, register_histogram, Counter, Histogram};
use log;

lazy_static::lazy_static! {
    pub static ref END_TO_END_LATENCY: Histogram = match register_histogram!(
        "loopix_end_to_end_latency_seconds",
        "End-to-end latency web proxy request.",
        vec![1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]
    ) {
        Ok(histogram) => {
            log::info!("END_TO_END_LATENCY histogram registered successfully.");
            histogram
        },
        Err(e) => {
            log::error!("Failed to register END_TO_END_LATENCY histogram: {:?}", e);
            Histogram::with_opts(prometheus::HistogramOpts::new(
                "loopix_end_to_end_latency_seconds",
                "End-to-end latency web proxy request."
            )).unwrap()
        }
    };

    pub static ref MIXNODE_DELAY: Histogram = match register_histogram!(
        "loopix_mixnode_delay_milliseconds",
        "Delay introduced by a mixnode.",
        vec![0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 2.75, 3.0, 3.25, 3.5, 3.75, 4.0, 6.0, 10.0, 50.0, 200.0]
        // vec![0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 6.0, 10.0, 50.0, 200.0]
    ) {
        Ok(histogram) => {
            log::info!("MIXNODE_DELAY histogram registered successfully.");
            histogram
        },
        Err(e) => {
            log::error!("Failed to register MIXNODE_DELAY histogram: {:?}", e);
            Histogram::with_opts(prometheus::HistogramOpts::new(
                "loopix_mixnode_delay_milliseconds",
                "Delay introduced by a mixnode."
            )).unwrap()
        }
    };

    pub static ref ENCRYPTION_LATENCY: Histogram = match register_histogram!(
        "loopix_encryption_latency_milliseconds",
        "Time taken for encryption.", // TODO maybe let's take all encryption latencies
        vec![0.1, 0.25, 0.5, 1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 2.75, 3.0, 3.25, 3.5, 3.75, 4.0, 10.0, 50.0, 200.0] 
    ) {
        Ok(histogram) => {
            log::info!("ENCRYPTION_LATENCY histogram registered successfully.");
            histogram
        },
        Err(e) => {
            log::error!("Failed to register ENCRYPTION_LATENCY histogram: {:?}", e);
            Histogram::with_opts(prometheus::HistogramOpts::new(
                "loopix_encryption_latency_milliseconds",
                "Time taken for encryption."
            )).unwrap()
        }
    };

    pub static ref DECRYPTION_LATENCY: Histogram = match register_histogram!(
        "loopix_decryption_latency_milliseconds",
        "Time taken for decryption.",
        vec![0.0001, 0.001, 0.0025, 0.005, 0.01, 0.5] // TODO maybe it makes more sense to do processing latency?
    ) {
        Ok(histogram) => {
            log::info!("DECRYPTION_LATENCY histogram registered successfully.");
            histogram
        },
        Err(e) => {
            log::error!("Failed to register DECRYPTION_LATENCY histogram: {:?}", e);
            Histogram::with_opts(prometheus::HistogramOpts::new(
                "loopix_decryption_latency_milliseconds",
                "Time taken for decryption."
            )).unwrap()
        }
    };

    pub static ref BANDWIDTH: Counter = match register_counter!(
        "loopix_bandwidth_bytes",
        "Bandwidth usage in bytes"
    ) {
        Ok(counter) => {
            log::info!("BANDWIDTH counter registered successfully.");
            counter
        },
        Err(e) => {
            log::error!("Failed to register BANDWIDTH counter: {:?}", e);
            Counter::new("loopix_bandwidth_bytes", "Bandwidth usage in bytes").unwrap()
        }
    };

    // TODO PROVIDER DELAY
    // TODO CLIENT DELAY
}

#[cfg(feature = "testing")]
pub mod testing;
