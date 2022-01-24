mod helpers;
use common::{
    broker::{BrokerMessage, ModulesMessage},
    node::modules::gossip_chat::GossipMessage,
};
use helpers::*;
use raw::gossip_chat::MessageIn;

#[test]
fn connect_nodes() {
    let _ = env_logger::init();
    let nbr_nodes = 10000;

    let mut net = Network::new();
    log::info!("Creating {nbr_nodes} nodes");
    net.add_nodes(nbr_nodes);
    let id = &net.nodes.keys().next().unwrap().clone();

    for step in 1..25 {
        log::info!("Process #{step}");
        net.process(1);

        // Search for messages
        let nbr = net
            .nodes
            .values_mut()
            .map(|node| node.messages())
            .reduce(|n, nbr| n + nbr)
            .unwrap();
        log::info!("Messages in network: #{nbr}");

        if step == 1 {
            net.send_message(
                id,
                BrokerMessage::Modules(ModulesMessage::Gossip(GossipMessage::MessageIn(
                    MessageIn::AddMessage(0., "msg".into()),
                ))),
            );        
        }
    }
}
