use std::error::Error;

use clap::{Parser, Subcommand};

use flarch::{
    data_storage::{DataStorage, DataStorageFile},
    tasks::wait_ms,
    web_rtc::connection::{ConnectionConfig, HostLogin, Login},
};
use flcrypto::access::Condition;
use flmodules::{
    dht_storage::core::RealmConfig,
    flo::{crypto::Rules, realm::FloRealm},
    network::{broker::NetworkIn, network_start, signal::SIGNAL_VERSION},
};
use flnode::{node::Node, version::VERSION_STRING};

/// Fledger node CLI binary
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the configuration directory
    #[arg(short, long, default_value = "./flnode")]
    config: String,

    /// Set the name of the node - reverts to a random value if not given
    #[arg(short, long)]
    name: Option<String>,

    /// Uptime interval - to stress test disconnections
    #[arg(short, long)]
    uptime_sec: Option<usize>,

    /// Signalling server URL
    #[arg(short, long, default_value = "wss://signal.fledg.re")]
    signal_url: String,

    /// Verbosity of the logger
    #[clap(flatten)]
    verbosity: clap_verbosity_flag::Verbosity,

    /// Log frequency in seconds
    #[arg(long, default_value = "5")]
    log_freq: u32,

    /// Log gossip messages
    #[arg(long, default_value = "false")]
    log_gossip: bool,

    /// Log random router
    #[arg(long, default_value = "false")]
    log_random: bool,

    /// Log countdown router
    #[arg(long, default_value = "false")]
    log_countdown: bool,

    /// Log dht-storage stats
    #[arg(long, default_value = "false")]
    log_dht_storage: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Handles Realms
    Realm {
        #[command(subcommand)]
        command: RealmCommands,
    },
    /// Lists cryptographic keys stored
    Crypto {},
    /// Prints the statistics of the different modules and then quits
    Stats {},
}

#[derive(Subcommand, Debug)]
enum RealmCommands {
    /// List available realms
    List,
    /// Creates a new realm
    Create {
        /// The name of the new realm.
        name: String,
        /// The maximum size of the sum of all the objects in the realm. The actual
        /// size will be bigger, as the data is serialized.
        max_space: Option<u64>,
        /// The maximum size of a single object in this realm.
        max_flo_size: Option<u32>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let mut logger = env_logger::Builder::new();
    logger.filter_module("fl", args.verbosity.clone().log_level_filter());
    logger.parse_env("RUST_LOG");
    logger.try_init().expect("Failed to initialize logger");

    let storage = DataStorageFile::new(args.config.clone(), "fledger".into());
    let mut node_config = Node::get_config(storage.clone_box())?;
    args.name
        .as_ref()
        .map(|name| node_config.info.name = name.clone());

    log::info!(
        "Starting app with version {}/{}",
        SIGNAL_VERSION,
        VERSION_STRING
    );

    log::debug!("Connecting to websocket at {}", args.signal_url);
    let network = network_start(
        node_config.clone(),
        ConnectionConfig::new(
            Some(args.signal_url.clone()),
            None,
            Some(HostLogin {
                url: "turn:web.fledg.re:3478".into(),
                login: Some(Login {
                    user: "something".into(),
                    pass: "something".into(),
                }),
            }),
        ),
    )
    .await?;
    let mut node = Node::start(Box::new(storage), node_config, network.broker).await?;
    let nc = &node.node_config.info;
    log::info!("Started node {}: {}", nc.get_id(), nc.name);

    match &args.command {
        Some(cmd) => match cmd {
            Commands::Realm { command } => match command {
                RealmCommands::List => list_realms(node, args).await?,
                RealmCommands::Create {
                    name,
                    max_space,
                    max_flo_size,
                } => {
                    log::info!("{name} / {max_space:?} / {max_flo_size:?}");
                    let fr = FloRealm::new(
                        name,
                        Rules::Update(Condition::Verifier(
                            node.crypto_storage
                                .get_signer()
                                .verifier()
                                .get_id(),
                        )),
                        RealmConfig {
                            max_space: max_space.unwrap_or(1000000),
                            max_flo_size: max_flo_size.unwrap_or(10000),
                        },
                    )?;

                    log::info!("Connecting to other nodes");
                    loop_node(&mut node, Some(&args), Some(2)).await?;

                    let mut ds = node.dht_storage.as_mut().expect("Need DHT-Storage").clone();
                    ds.store_flo(fr.into())?;
                    ds.propagate()?;

                    list_realms(node, args).await?;
                }
            },
            Commands::Crypto {} => todo!(),
            Commands::Stats {} => todo!(),
        },
        None => loop_node(&mut node, Some(&args), None).await?,
    }

    Ok(())
}

async fn list_realms(mut node: Node, args: Args) -> Result<(), Box<dyn Error>> {
    let mut ds = node.dht_storage.as_mut().expect("Need DHT-Storage").clone();
    log::info!("Waiting for update of data");
    loop_node(&mut node, Some(&args), Some(20)).await?;
    log::info!("Requesting sync of DHT-storage and waiting for answers");
    ds.sync()?;
    ds.propagate()?;
    loop_node(&mut node, Some(&args), Some(2)).await?;
    let rids = ds.get_realm_ids().await?;
    if rids.len() == 0 {
        log::info!("No realms found.");
        return Ok(());
    }
    log::info!(
        "Realm-IDs are: {}",
        rids.iter()
            .map(|rid| format!("{rid}"))
            .collect::<Vec<_>>()
            .join(" :: ")
    );

    Ok(())
}

async fn loop_node(
    node: &mut Node,
    args: Option<&Args>,
    max_count: Option<u32>,
) -> Result<(), Box<dyn Error>> {
    let mut i: u32 = 0;
    node.broker_net
        .emit_msg_in(NetworkIn::WSUpdateListRequest)?;
    loop {
        i += 1;

        wait_ms(1000).await;

        if let Some(a) = args.as_ref() {
            a.log(i, node).await?;
        }

        if let Some(c) = max_count.as_ref() {
            if c < &i {
                return Ok(());
            }
        }
    }
}

impl Args {
    async fn log(&self, i: u32, node: &mut Node) -> Result<(), Box<dyn Error>> {
        if i % self.log_freq == self.log_freq - 1 {
            if self.log_random {
                log::info!(
                    "Nodes are: {}",
                    node.nodes_online()?
                        .iter()
                        .map(|n| format!("{}/{}", n.name, n.get_id()))
                        .collect::<Vec<_>>()
                        .join(" - ")
                );
            }

            if self.log_countdown {
                let ping = &node.ping.as_ref().unwrap().storage.borrow();
                log::info!("Nodes countdowns are: {:?}", ping.stats);
            }

            if self.log_gossip {
                log::debug!(
                    "Chat messages are: {:?}",
                    node.gossip.as_ref().unwrap().chat_events()
                );
            }

            if self.log_dht_storage {
                if let Some(ds) = node.dht_storage.as_mut() {
                    let rids = ds.get_realm_ids().await?;
                    if rids.len() == 0 {
                        log::info!("No realms found.");
                        return Ok(());
                    }
                    log::info!(
                        "Realm-IDs are: {}",
                        rids.iter()
                            .map(|rid| format!("{rid}"))
                            .collect::<Vec<_>>()
                            .join(" :: ")
                    );
                }
            }
        }

        Ok(())
    }
}
