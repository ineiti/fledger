use std::{fs::File, str::FromStr, time::Duration};

use clap::Parser;
use flarch::web_rtc::web_socket_server::WebSocketServer;
use flmodules::{
    flo::realm::RealmID,
    network::signal::{SignalConfig, SignalOut, SignalServer},
};
use metrics_exporter_influx::{InfluxBuilder, InfluxRecorderHandle};

/// Fledger signalling server
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Verbosity
    #[clap(flatten)]
    verbosity: clap_verbosity_flag::Verbosity<clap_verbosity_flag::InfoLevel>,

    /// System realm - if this is set, no other realms are allowed by default.
    #[arg(long)]
    system_realm: Option<String>,

    /// Maximum list size to return when a node asks for the list of other nodes.
    #[arg(long)]
    max_list_len: Option<usize>,
}

fn setup_metrics(node_name: String) -> InfluxRecorderHandle {
    log::info!("Setting up metrics");
    let metrics_file = File::create(format!("/tmp/{}.metrics", node_name))
        .expect(format!("could not create /tmp/{}.metrics", node_name).as_ref());
    return InfluxBuilder::new()
        .with_duration(Duration::from_secs(10))
        .with_writer(metrics_file)
        .add_global_tag("node_name", node_name)
        .install()
        .expect("could not setup influx recorder");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let mut logger = env_logger::Builder::new();
    logger.filter_level(args.verbosity.log_level_filter());
    logger.parse_env("RUST_LOG");
    logger.try_init().expect("Failed to initialize logger");

    let _influx = setup_metrics("flsignal".into());

    let wss = WebSocketServer::new(8765).await?;
    let system_realm = args.system_realm.and_then(|sr| RealmID::from_str(&sr).ok());
    log::info!("System realm config is: {:?}", system_realm);
    let mut signal_server = SignalServer::new(
        wss,
        SignalConfig {
            ttl_minutes: 2,
            system_realm,
            max_list_len: args.max_list_len,
        },
    )
    .await?;
    let (msgs, _) = signal_server.get_tap_out_sync().await?;

    log::info!("Started listening on port 8765");
    for msg in msgs {
        log::trace!("{:?}", msg);
        if matches!(msg, SignalOut::Stopped) {
            log::error!("Server stopped working - exiting");
            return Ok(());
        }
    }
    Ok(())
}
