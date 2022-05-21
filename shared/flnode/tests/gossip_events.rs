use flarch::start_logging;
use flmodules::{gossip_events::events, nodeids::U256};
use flnode::node_data::Brokers;

mod helpers;
use helpers::*;

#[tokio::test]
async fn gossip_2() -> Result<(), NetworkError> {
    gossip(2).await?;
    Ok(())
}

#[tokio::test]
async fn gossip_30() -> Result<(), NetworkError> {
    gossip(30).await?;
    Ok(())
}

async fn gossip(nbr_nodes: usize) -> Result<(), NetworkError> {
    start_logging();

    let mut net = Network::new();
    log::info!("Creating {nbr_nodes} nodes");
    net.add_nodes(Brokers::ENABLE_ALL, nbr_nodes).await?;
    let id = &net.nodes.keys().next().unwrap().clone();
    // Hand-crafted formula that works for 2, 30, 100 nodes
    let steps = ((nbr_nodes as f64).log10() + 4.5) as i32;

    log::info!("Adding 1st message");
    add_chat_message(&mut net, id, 0).await;

    for step in 0..=steps * 3 {
        log::debug!("Process #{step}");
        net.process(1).await;

        // Search for messages
        let nbr = net
            .nodes
            .values_mut()
            .map(|node| {
                node.node_data.update();
                node.messages()
            })
            .reduce(|n, nbr| n + nbr)
            .unwrap();
        log::debug!("Messages in network: #{nbr}");

        match step {
            s if s == steps => {
                log::info!(
                    "Checking messages {} == {nbr} and adding 2nd message",
                    nbr_nodes
                );
                assert_eq!(nbr_nodes, nbr);
                add_chat_message(&mut net, id, step).await;
            }
            s if s == (2 * steps) => {
                log::info!(
                    "Checking messages {} == {nbr} and adding {nbr_nodes} nodes",
                    2 * nbr_nodes
                );
                assert_eq!(2 * nbr_nodes, nbr);
                net.add_nodes(Brokers::ENABLE_ALL, nbr_nodes).await?;
            }
            s if s == steps * 3 => {
                log::info!("Checking messages {} == {nbr}", 4 * nbr_nodes);
                assert_eq!(4 * nbr_nodes, nbr);
            }
            _ => {}
        }
    }

    Ok(())
}

async fn add_chat_message(net: &mut Network, id: &U256, step: i32) {
    let msg = events::Event {
        category: events::Category::TextMessage,
        src: U256::rnd(),
        created: step as f64,
        msg: "msg".into(),
    };
    let node = net.nodes.get_mut(id).expect("getting node");
    node.node_data
        .gossip
        .add_event(msg)
        .await
        .expect("adding new event");
}
