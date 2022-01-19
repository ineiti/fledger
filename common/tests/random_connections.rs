mod helpers;
use helpers::*;

#[test]
fn connect_nodes() {
    let _ = env_logger::init();

    let mut net = Network::new();
    net.add_nodes(3);
    net.tick();
    net.tick();
    net.tick();
    net.tick();
    net.tick();
}