use std::{error::Error, time::Duration};

use flarch::start_logging_filter_level;

#[cfg(test)]
use flmodules::{
    loopix::testing::{LoopixSetup, NetworkSimul, ProxyBroker},
    overlay::messages::{NetworkWrapper, OverlayIn, OverlayMessage},
};
use serde::{Deserialize, Serialize};

/**
 * This test sets up a number of Loopix nodes: clients, mixers, and providers.
 * You can either let it use the proxies or send a simple message through Loopix.
 * Start with the simple message!
 *
 * Some comments about the Loopix-implementation:
 * - it is not clear to me how the NodeID as u32 maps to different clients, mixers,
 *  and providers. Also, this will fail in the current implementation, as it uses
 *  NodeIDs provided by the system which will not follow your numbering schema.
 *  So you need to change the `LoopixConfig` to allow for random NodeIDs.
 * - for the same reason, passing NodeID as u32 to `default_with_path_length`
 *  is a very bad idea, as the real system will use random NodeIDs.
 * - I realize now that the setup of the brokers is really confusing - so I take some
 *  blame for your `LoopixBroker::start`. And it also shows that you had trouble
 *  implementing the `LoopixTranslate`... What you should do is to remove the
 *  `overlay: Broker<OverlayMessage>` from the `start` method. This broker is provided
 *  by the `OverlayLoopix::start` method, which takes in the `Broker<LoopixMessage>`.
 *  Happy to discuss this asynchronously over slack...
 */
#[tokio::test]
async fn test_loopix() -> Result<(), Box<dyn Error>> {
    start_logging_filter_level(vec![], log::LevelFilter::Trace);

    let loopix_setup = LoopixSetup::new(2).await?;
    let mut network = NetworkSimul::new();
    network.add_nodes(loopix_setup.clients.clone()).await?;
    network.add_nodes(loopix_setup.mixers.clone()).await?;
    network.add_nodes(loopix_setup.providers.clone()).await?;

    let stop = network.process_loop();

    let mut proxy_src = ProxyBroker::new(loopix_setup.clients[0].loopix.clone()).await?;
    let proxy_dst = ProxyBroker::new(loopix_setup.clients[1].loopix.clone()).await?;

    tokio::time::sleep(Duration::from_secs(3)).await;

    let resp_result = proxy_src.proxy.get_with_timeout("https://fledg.re/", Duration::from_secs(30)).await;

    match resp_result {
        Ok(mut resp) => {
            log::debug!("Got response struct with headers: {resp:?}");
            let content = resp.text().await.expect("getting text");
            log::debug!("Got text from content: {content:?}");
        }
        Err(e) => {
            log::error!("Failed to get response: {e}");
            loopix_setup.print_all_messages(true).await;
            return Err(e.into());
        }
    }

    println!("Ids for proxies: ${} / ${}", proxy_src.id, proxy_dst.id);

    stop.send(true).ok();

    loopix_setup.print_all_messages(true).await;

    Ok(())
}

#[tokio::test]
async fn test_tiny_loopix() -> Result<(), Box<dyn Error>> {
    start_logging_filter_level(vec![], log::LevelFilter::Trace);

    let mut loopix_setup = LoopixSetup::new(2).await?;
    let mut network = NetworkSimul::new();
    network.add_nodes(loopix_setup.clients.clone()).await?;
    network.add_nodes(loopix_setup.mixers.clone()).await?;
    network.add_nodes(loopix_setup.providers.clone()).await?;

    let stop = network.process_loop();

    // Send a message from a client node to a provider:
    let id_dst = loopix_setup.clients[1].config.info.get_id();
    loopix_setup.clients[0]
        .overlay
        .emit_msg(OverlayMessage::Input(OverlayIn::NetworkWrapperToNetwork(
            id_dst,
            NetworkWrapper::wrap_yaml(
                "Test",
                &TestMessage {
                    field: "secret message".into(),
                },
            )?,
        )))?;

    tokio::time::sleep(Duration::from_secs(10)).await;

    loopix_setup.print_all_messages(true).await;

    // Quit the tokio-thread
    stop.send(true).ok();

    Ok(())
}

/**
 * Random message to be sent from a client to a provider.
 */
#[derive(Debug, Serialize, Deserialize)]
struct TestMessage {
    field: String,
}
