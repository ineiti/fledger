use clap::Parser;

use flarch::{
    data_storage::{DataStorage, DataStorageFile},
    start_logging_filter,
    tasks::wait_ms,
};
use flnet_libc::network_start;
use flnode::node::Node;

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
}

const VERSION_STRING: &str = "123";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    start_logging_filter(vec!["fl"]);

    let args = Args::parse();
    let storage = DataStorageFile::new(args.config, "fledger".into());
    let mut node_config = Node::get_config(storage.clone())?;
    args.name.map(|name| node_config.info.name = name);

    log::info!("Starting app with version {}", VERSION_STRING);

    log::debug!("Connecting to websocket at {}", args.signal_url);
    let network = network_start(node_config.clone(), &args.signal_url).await?;
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
            let ping = &node.ping.storage;
            log::info!("Nodes countdowns are: {:?}", ping.stats);
            log::info!("Chat messages are: {:?}", node.gossip.chat_events());
        }
        wait_ms(1000).await;
    }
}
