use clap::{Parser, ValueEnum};
use std::str::FromStr;
use std::time::Duration;
use tokio_stream::StreamExt;

use flarch::{
    broker::BrokerError,
    nodeids::U256,
    start_logging_filter,
    tasks::{wait_ms, Interval},
    web_rtc::connection::ConnectionConfig,
};
use flmodules::nodeconfig::NodeConfig;
use flnet::network::NetReply;

#[derive(Parser, Debug)]
struct Args {
    /// Which shared code to run
    #[arg(value_enum, short, long, default_value_t = Action::Ping)]
    action: Action,
    #[arg(short, long)]
    server_id: Option<String>,
}

#[derive(ValueEnum, Clone, Debug)]
enum Action {
    Ping,
    Server,
    Client,
}

#[tokio::main]
async fn main() -> Result<(), BrokerError> {
    start_logging_filter(vec!["fl", "ping"]);
    let args = Args::parse();

    match args.action {
        Action::Ping => ping().await?,
        Action::Server => server().await?,
        Action::Client => client(&args.server_id.expect("need server_id").replace("-", "")).await?,
    }
    Ok(())
}

async fn ping() -> Result<(), BrokerError> {
    // Create a random node-configuration. It uses serde for easy serialization.
    let nc = NodeConfig::new();
    log::info!("Our node-ID is {:?}", nc.info.get_id());
    // Connect to the signalling server and wait for connection requests.
    let mut net = flnet::network_start(
        nc.clone(),
        ConnectionConfig::from_signal("ws://localhost:8765"),
    )
    .await
    .expect("Setting up network");
    // Create a timer that fires every second
    let mut interval_sec = Interval::new_interval(Duration::from_secs(5));

    loop {
        tokio::select! {
            msg = net.recv() => {
                    match msg {
                        // Display the messages received
                        NetReply::RcvNodeMessage(from, msg) => log::info!("Got message {msg:?} from node {from}"),
                        // If a new list is available, ping all nodes in the list
                        NetReply::RcvWSUpdateList(list) => for node in list {
                            if node.get_id() != nc.info.get_id() {
                                // Sends a text message to the 'node' if it's not ourselves
                                net.send_msg(node.get_id(), "Ping".into())?
                            }
                        },
                        _ => {}
                }
            },
            // Send a request for a list of all nodes once per second.
            _ = interval_sec.next() => net.send_list_request()?,
        }
    }
}

async fn server() -> Result<(), BrokerError> {
    // Create a random node-configuration. It uses serde for easy serialization.
    let nc = NodeConfig::new();
    // Connect to the signalling server and wait for connection requests.
    let mut net = flnet::network_start(
        nc.clone(),
        ConnectionConfig::from_signal("ws://localhost:8765"),
    )
    .await
    .expect("Starting network");

    // Print our ID so it can be copied to the server
    log::info!("Our ID is: {:?}", nc.info.get_id());
    loop {
        // Wait for messages and print them to stdout.
        log::info!("Got message: {:?}", net.recv().await);
    }
}

async fn client(server_id: &str) -> Result<(), BrokerError> {
    // Create a random node-configuration. It uses serde for easy serialization.
    let nc = NodeConfig::new();
    // Connect to the signalling server and wait for connection requests.
    let mut net = flnet::network_start(
        nc.clone(),
        ConnectionConfig::from_signal("ws://localhost:8765"),
    )
    .await
    .expect("Starting network");

    // Need to get the client-id from the message in client()
    let server_id = U256::from_str(server_id).expect("get client id");
    log::info!("Server id is {server_id:?}");
    // This sends the message by setting up a connection using the signalling server.
    // The client must already be running and be registered with the signalling server.
    // Using `SendNodeMessage` will set up a connection using the signalling server, but
    // in the best case, the signalling server will not be used anymore afterwards.
    net.send_msg(server_id, "ping".into())?;

    // Wait for the connection to be set up and the message to be sent.
    wait_ms(1000).await;

    Ok(())
}
