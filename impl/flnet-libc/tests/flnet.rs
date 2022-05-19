use flnet::{
    config::NodeConfig,
    network::{NetCall, NetReply, Network, NetworkMessage},
    signal::{dummy::WebSocketSimul, server::SignalServer},
};
use flnet_libc::web_rtc_setup::WebRTCConnectionSetupLibc;
use thiserror::Error;

#[derive(Debug, Error)]
enum TestError {
    #[error(transparent)]
    Network(#[from] flnet::network::NetworkError),
    #[error(transparent)]
    Broker(#[from] flutils::broker::BrokerError),
}

#[tokio::test(flavor = "multi_thread")]
async fn test_nodes() -> Result<(), TestError> {
    flutils::start_logging_filter(vec!["flnet", "flnet_libc", "flutils"]);

    let mut wss_simul = WebSocketSimul::new();
    let _signal_server = SignalServer::new(wss_simul.server.clone(), 16).await?;
    let nc1 = NodeConfig::new();
    let nc2 = NodeConfig::new();

    let (index1, ws1) = wss_simul.new_connection().await?;

    log::debug!("Starting network 1");
    let mut net1 = Network::start(
        nc1.clone(),
        ws1.clone(),
        Box::new(|| Box::new(Box::pin(WebRTCConnectionSetupLibc::new_box()))),
    )
    .await?;
    wss_simul.announce_connection(index1).await?;

    log::debug!("Starting network 2");
    let (index2, ws2) = wss_simul.new_connection().await?;
    let mut net2 = Network::start(
        nc2.clone(),
        ws2,
        Box::new(|| Box::new(Box::pin(WebRTCConnectionSetupLibc::new_box()))),
    )
    .await?;
    wss_simul.announce_connection(index2).await?;

    log::debug!("Sending 1st message");
    let (tap_2, id2) = net2.get_tap().await?;
    net1.emit_msg(NetCall::SendNodeMessage((nc2.our_node.get_id(), "hello".to_string())).into())
        .await?;

    log::debug!("Waiting for message to go through");
    for msg in tap_2 {
        log::info!("Got message: {msg:?}");
        if let NetworkMessage::Reply(NetReply::RcvNodeMessage((id, msg_str))) = msg {
            log::debug!("Got NodeMessage: {id}-{msg_str}");
            if msg_str == "hello".to_string() {
                net2.remove_subsystem(id2).await?;
                break;
            }
        }
    }

    let (tap_1, id1) = net1.get_tap().await?;
    net2.emit_msg(NetCall::SendNodeMessage((nc1.our_node.get_id(), "there".to_string())).into())
        .await?;

    for msg in tap_1 {
        log::info!("Got message: {msg:?}");
        if let NetworkMessage::Reply(NetReply::RcvNodeMessage((_, msg_str))) = msg {
            if msg_str == "there".to_string() {
                net1.remove_subsystem(id1).await?;
                break;
            }
        }
    }

    Ok(())
}
