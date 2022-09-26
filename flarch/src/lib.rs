pub mod data_storage;

#[cfg(feature = "wasm")]
pub mod tasks {
    mod wasm;
    pub use wasm::*;
}

#[cfg(not(feature = "wasm"))]
pub mod tasks {
    mod libc;
    pub use libc::*;
}

pub use tasks::*;

pub fn start_logging() {
    start_logging_filter(vec![]);
}

pub fn start_logging_filter(filters: Vec<&str>) {
    start_logging_filter_level(filters, log::LevelFilter::Debug);
}

pub fn start_logging_filter_level(filters: Vec<&str>, level: log::LevelFilter) {
    let mut logger = env_logger::Builder::new();
    if filters.len() == 0 {
        logger.filter_level(level);
    } else {
        for filter in filters {
            logger.filter_module(filter, level);
        }
    }
    logger.parse_env("RUST_LOG");
    if logger.try_init().is_err(){
        log::trace!("Logger probably already initialized");
    }
}
