mod helpers;
use common::{
    broker::{BrokerMessage, ModulesMessage},
    node::modules::gossip_chat::GossipMessage,
};
use helpers::*;
use raw::gossip_chat::MessageIn;
use types::nodeids::U256;

#[test]
fn connect_nodes() {
    let _ = env_logger::init();
    let nbr_nodes = 100;

    let mut net = Network::new();
    log::info!("Creating {nbr_nodes} nodes");
    net.add_nodes(nbr_nodes);
    let id = &net.nodes.keys().next().unwrap().clone();

    for step in 1..50 {
        log::debug!("Process #{step}");
        net.process(1);

        // Search for messages
        let nbr = net
            .nodes
            .values_mut()
            .map(|node| node.messages())
            .reduce(|n, nbr| n + nbr)
            .unwrap();
        log::debug!("Messages in network: #{nbr}");

        match step {
            1 => {
                log::info!("Adding 1st message");
                add_message(&mut net, id, step);
            }
            20 => {
                log::info!("Checking messages {} == {nbr} and adding 2nd message", nbr_nodes);
                assert_eq!(nbr_nodes, nbr);
                add_message(&mut net, id, step);
            }
            30 => {
                log::info!("Checking messages {} == {nbr} and adding {nbr_nodes} nodes", 2 * nbr_nodes);
                assert_eq!(2 * nbr_nodes, nbr);
                net.add_nodes(nbr_nodes);
            }
            45 => {
                log::info!("Checking messages {} == {nbr}", 4 * nbr_nodes);
                assert_eq!(4 * nbr_nodes, nbr);
            }
            _ => {}
        }
    }
}

fn add_message(net: &mut Network, id: &U256, step: i32) {
    net.send_message(
        id,
        BrokerMessage::Modules(ModulesMessage::Gossip(GossipMessage::MessageIn(
            MessageIn::AddMessage(step as f64, "msg".into()),
        ))),
    );
}
