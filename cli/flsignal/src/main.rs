use flnet::signal::server::SignalServer;
use flnet_libc::web_socket_server::WebSocketServer;
use flutils::start_logging;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    start_logging();

    let wss = WebSocketServer::new(8765).await?;
    let mut signal_server = SignalServer::new(wss, 16).await?;
    let (msgs, _) = signal_server.get_tap().await?;

    log::info!("Started listening on port 8765");
    for msg in msgs {
        log::debug!("{:?}", msg);
    }
    Ok(())
}
