use clap::Parser;
use flarch::web_rtc::web_socket_server::WebSocketServer;
use flmodules::network::signal::{SignalOut, SignalServer};

/// Fledger signalling server
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Verbosity
    #[clap(flatten)]
    verbosity: clap_verbosity_flag::Verbosity,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let mut logger = env_logger::Builder::new();
    logger.filter_level(args.verbosity.log_level_filter());
    logger.parse_env("RUST_LOG");
    logger.try_init().expect("Failed to initialize logger");

    let wss = WebSocketServer::new(8765).await?;
    let mut signal_server = SignalServer::new(wss, 2).await?;
    let (msgs, _) = signal_server.get_tap_out_sync().await?;

    log::info!("Started listening on port 8765");
    for msg in msgs {
        log::debug!("{:?}", msg);
        if matches!(msg, SignalOut::Stopped) {
            log::error!("Server stopped working - exiting");
            return Ok(());
        }
    }
    Ok(())
}
