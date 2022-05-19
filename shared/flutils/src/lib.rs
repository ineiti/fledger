pub mod broker;
pub mod data_storage;
pub mod nodeids;

#[cfg(feature = "wasm")]
pub mod arch {
    mod wasm;
    pub use wasm::*;
}

#[cfg(not(feature = "wasm"))]
pub mod arch {
    mod libc;
    pub use libc::*;
}

pub use arch::*;

pub fn start_logging() {
    start_logging_filter(vec![]);
}

pub fn start_logging_filter(filters: Vec<&str>) {
    let mut logger = env_logger::Builder::new();
    if filters.len() == 0 {
        logger.filter_level(log::LevelFilter::Debug);
    } else {
        for filter in filters {
            logger.filter_module(filter, log::LevelFilter::Debug);
        }
    }
    logger.parse_env("RUST_LOG");
    logger.try_init().expect("Failed to initialize logger");
}
