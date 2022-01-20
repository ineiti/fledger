mod helpers;
use helpers::*;

#[test]
fn connect_nodes() {
    let _ = env_logger::init();

    let mut net = Network::new();
    log::info!("Creating nodes");
    net.add_nodes(1000);
    log::info!("Processing 5 times");
    net.process(5);
}