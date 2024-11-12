use std::error::Error;

use flarch::{start_logging, tasks::now};
mod helpers;
use flmodules::{
    gossip_events::core::{Category, Event},
    loopix::{broker::LoopixBroker, testing::LoopixSetup},
    Modules,
};
use helpers::*;

/**
 * A more realistic test using the real nodes.
 * 1. it sets up the nodes without loopix
 * 2. one node creates a configuration for all the nodes (including the private
 * keys...)
 * 3. this node puts the configuration in the gossip-module
 * 4. once all nodes got the configuration, every node initializes its
 * loopix-module with the received configuraion.
 */
async fn proxy_nodes_n(path_length: usize) -> Result<(), Box<dyn Error>> {
    // Setting up nodes with randon IDs.
    let nbr_nodes = path_length * (path_length + 2);
    let mut net = NetworkSimul::new();
    log::info!("Creating {nbr_nodes} nodes");
    net.add_nodes(Modules::all(), nbr_nodes).await?;
    net.process(5).await;
    log::info!("Sent a total of {} messages", net.messages);

    // Create the global configuration and propagate it with the gossip-module.
    let loopix_setup = LoopixSetup::new(2).await?;
    if let Some((k, v)) = net.nodes.iter_mut().next() {
        if let Some(g) = v.node.gossip.as_mut() {
            let event = Event {
                category: Category::TextMessage,
                src: *k,
                created: now(),
                msg: serde_yaml::to_string(&loopix_setup)?,
            };
            g.add_event(event).await?;
        }
    }
    // Run the network until all messages are propagated.
    net.process(2 * path_length).await;

    // Suppose every node has the configuration now and can initialize its loopix
    // module.
    for (id, v) in net.nodes.iter_mut() {
        let setup: LoopixSetup = serde_yaml::from_str(
            &v.node
                .gossip
                .as_ref()
                .unwrap()
                .events(Category::TextMessage)
                .first()
                .unwrap()
                .msg,
        )?;
        // This is my configuration-wrapper. Of course the nodes should have a way to get their role.
        // The role choice could be done in step 2 where one node creates the global configuration.
        let config = setup
            .get_config(*id, flmodules::loopix::config::LoopixRole::Client)
            .await?;
        v.node.loopix = Some(LoopixBroker::start(v.node.broker_net.clone(), config).await?);
        //
        // v.node.webproxy = Some(WebProxy::start(v.node.storage, *id, OverlayLoopix::start(v.node.loopix.unwrap().clone()), WebProxyConfig::default()).await?);
    }

    // Now do some webProxy stuff while the network runs.
    // net.nodes.clients[1].webproxy.get("google.com");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connect_nodes() -> Result<(), Box<dyn Error>> {
        start_logging();

        proxy_nodes_n(4).await?;
        Ok(())
    }
}
