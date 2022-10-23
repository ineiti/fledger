use clap::Parser;

use flarch::{
    data_storage::{DataStorage, DataStorageFile},
    tasks::wait_ms,
};
use flnet::signal::SIGNAL_VERSION;
use flnet::network_broker_start;
use flnode::{node::Node, version::VERSION_STRING};

/// Fledger node CLI binary
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Path to the configuration directory
    #[clap(short, long, default_value = "./flnode")]
    config: String,

    /// Set the name of the node - reverts to a random value if not given
    #[clap(short, long)]
    name: Option<String>,

    /// Uptime interval - to stress test disconnections
    #[clap(short, long)]
    uptime_sec: Option<usize>,

    /// Signalling server URL
    #[clap(short, long, default_value = "wss://signal.fledg.re")]
    signal_url: String,

    /// Verbosity of the logger
    #[clap(flatten)]
    verbosity: clap_verbosity_flag::Verbosity,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let mut logger = env_logger::Builder::new();
    logger.filter_module("fl", args.verbosity.log_level_filter());
    logger.parse_env("RUST_LOG");
    logger.try_init().expect("Failed to initialize logger");

    let storage = DataStorageFile::new(args.config, "fledger".into());
    let mut node_config = Node::get_config(storage.clone())?;
    args.name.map(|name| node_config.info.name = name);

    log::info!(
        "Starting app with version {}/{}",
        SIGNAL_VERSION,
        VERSION_STRING
    );

    log::debug!("Connecting to websocket at {}", args.signal_url);
    let network = network_broker_start(node_config.clone(), &args.signal_url).await?;
    let mut node = Node::start(Box::new(storage), node_config, network).await?;
    let nc = &node.node_config.info;
    log::info!("Starting node {}: {}", nc.get_id(), nc.name);

    log::info!("Started successfully");
    let mut i: i32 = 0;
    loop {
        i += 1;
        node.process()
            .await
            .err()
            .map(|e| log::warn!("Couldn't process node: {e:?}"));

        if i % 3 == 2 {
            log::info!("Nodes are: {:?}", node.nodes_online()?);
            let ping = &node.ping.as_ref().unwrap().storage;
            log::info!("Nodes countdowns are: {:?}", ping.stats);
            log::debug!("Chat messages are: {:?}", node.gossip.as_ref().unwrap().chat_events());
        }
        wait_ms(1000).await;
    }
}
