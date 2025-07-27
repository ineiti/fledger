use flarch::{nodeids::U256, start_logging_filter_level};
use flmodules::{gossip_events::core, Modules};

mod helpers;
use helpers::*;

async fn gossip(nbr_nodes: usize) -> anyhow::Result<()> {
    start_logging_filter_level(vec![], log::LevelFilter::Info);

    let mut net = NetworkSimul::new();
    log::info!("Creating {nbr_nodes} nodes");
    net.add_nodes(Modules::stable(), nbr_nodes).await?;
    let id = &net.nodes.keys().next().unwrap().clone();
    // Hand-crafted formula that works for 2, 30, 100 nodes
    let steps = ((nbr_nodes as f64).log10() + 5.0) as i32;

    log::info!("Adding 1st message");
    add_chat_message(&mut net, id, 0).await;

    for step in 0..=steps * 3 {
        log::debug!("Process #{step}");
        net.process(1).await;

        // Search for messages
        let nbr = net
            .nodes
            .values_mut()
            .map(|node_timer| {
                node_timer.messages()
            })
            .reduce(|n, nbr| n + nbr)
            .unwrap();
        log::debug!("Messages in network: #{nbr}");

        match step {
            s if s == steps => {
                log::info!("Checking messages {nbr_nodes} == {nbr} and adding 2nd message");
                assert_eq!(nbr_nodes, nbr);
                add_chat_message(&mut net, id, step).await;
            }
            s if s == (2 * steps) => {
                log::info!(
                    "Checking messages {} == {nbr} and adding {nbr_nodes} nodes",
                    2 * nbr_nodes
                );
                assert_eq!(2 * nbr_nodes, nbr);
                net.add_nodes(Modules::stable(), nbr_nodes).await?;
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

async fn add_chat_message(net: &mut NetworkSimul, id: &U256, step: i32) {
    let msg = core::Event {
        category: core::Category::TextMessage,
        src: U256::rnd(),
        created: step as i64,
        msg: "msg".into(),
    };
    let node_timer = net.nodes.get_mut(id).expect("getting node");
    node_timer
        .node
        .gossip
        .as_mut()
        .unwrap()
        .add_event(msg)
        .await
        .expect("adding new event");
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn gossip_2() -> anyhow::Result<()> {
        gossip(2).await?;
        Ok(())
    }

    // 30 nodes have trouble correctly shutting down, so we keep it to 20 for now.
    #[tokio::test]
    async fn gossip_10() -> anyhow::Result<()> {
        gossip(10).await?;
        Ok(())
    }
}
