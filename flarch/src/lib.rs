// pub mod broker;
pub mod broker;
pub mod data_storage;
pub mod nodeids;
pub mod tasks;
pub mod web_rtc;

pub fn start_logging() {
    start_logging_filter(vec![]);
}

pub fn start_logging_filter(filters: Vec<&str>) {
    start_logging_filter_level(filters, log::LevelFilter::Info);
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
    if logger.try_init().is_err() {
        log::trace!("Logger probably already initialized");
    }
}

pub fn log_backtrace() -> String {
    let backtrace = std::backtrace::Backtrace::force_capture();
    let backtrace = btparse::deserialize(&backtrace).unwrap();
    backtrace
        .frames
        .iter()
        .skip(3)
        .map(|f| format!("{f:?}"))
        .take(9)
        .collect::<Vec<_>>()
        .join("\n")
}

pub use flmacro::platform_async_trait;
pub use rand::random;
