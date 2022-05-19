use clap::Parser;

use flnet_libc::{
    data_storage::DataStorageFile,
    // data_storage::DataStorageFile,
    web_rtc_setup::web_rtc_spawner,
    web_socket_client::WebSocketClient,
};
use flnode::{node::Node, node_data::NodeData};
use flutils::{data_storage::DataStorageBase, start_logging_filter, arch::wait_ms};

/// Fledger node CLI binary
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Path to the configuration directory
    #[clap(short, long, default_value = "./fledger")]
    config: String,

    /// Set the name of the node - reverts to a random value if not given
    #[clap(short, long)]
    name: Option<String>,

    /// Uptime interval - to stress test disconnections
    #[clap(short, long)]
    uptime_sec: Option<usize>,
}

const VERSION_STRING: &str = "123";
const URL: &str = "ws://localhost:8765";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    start_logging_filter(vec!["fl"]);

    let args = Args::parse();
    let storage = DataStorageFile::new(args.config);
    let mut node_config = NodeData::get_config(storage.clone())?;
    args.name.map(|name| node_config.our_node.info = name);

    log::info!("Starting app with version {}", VERSION_STRING);

    log::debug!("Connecting to websocket at {URL}");
    let ws = WebSocketClient::connect(URL)
        .await
        .expect("Failed to connect to signalling server");

    let mut node = Node::new(Box::new(storage), node_config, "cli", ws, web_rtc_spawner()).await?;
    let nc = node.info();
    log::info!("Starting node {}: {}", nc.get_id(), nc.info);

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
            let ping = node.nodes_ping();
            log::info!("Nodes countdowns are: {:?}", ping.stats);
            log::info!("Chat messages are: {:?}", node.get_chat_messages());
        }
        wait_ms(1000).await;
    }
}
