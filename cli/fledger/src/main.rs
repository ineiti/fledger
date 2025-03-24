use clap::{Parser, Subcommand};

use flarch::{
    data_storage::{DataStorage, DataStorageFile},
    tasks::wait_ms,
    web_rtc::connection::{ConnectionConfig, HostLogin},
};
use flmodules::{
    dht_storage::broker::DHTStorage,
    network::{broker::NetworkIn, network_start, signal::SIGNAL_VERSION},
};
use flnode::{node::Node, version::VERSION_STRING};
use page::{Page, PageCommands};
use realm::{RealmCommands, RealmHandler};

mod page;
mod realm;

/// Fledger node CLI binary
#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Args {
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

    /// Stun server - by default points to Google's
    #[arg(long, default_value = "stun:stun.l.google.com:19302")]
    stun_url: String,

    /// Turn server - by default points to fledger's open Turn server.
    /// username:password@turn:domain:port
    #[arg(long, default_value = "something:something@turn:web.fledg.re:3478")]
    turn_url: String,

    /// Disables all stun and turn servers for local tests
    #[arg(long, default_value = "false")]
    disable_turn_stun: bool,

    /// Verbosity of the logger
    #[clap(flatten)]
    verbosity: clap_verbosity_flag::Verbosity<clap_verbosity_flag::InfoLevel>,

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

#[derive(Subcommand, Debug, Clone)]
enum Commands {
    /// Handles Realms
    Realm {
        #[command(subcommand)]
        command: RealmCommands,
    },
    /// Adds, removes, and edits pages
    Page {
        #[command(subcommand)]
        command: PageCommands,
    },
    /// Lists cryptographic keys stored
    Crypto {},
    /// Prints the statistics of the different modules and then quits
    Stats {},
}

struct Fledger {
    node: Node,
    ds: DHTStorage,
    args: Args,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    let cc = if args.disable_turn_stun {
        ConnectionConfig::from_signal(&args.signal_url)
    } else {
        ConnectionConfig::new(
            args.signal_url.clone().into(),
            Some(HostLogin::from_url(&args.stun_url)),
            Some(HostLogin::from_login_url(&args.turn_url)?),
        )
    };
    log::debug!("Connecting to websocket at {:?}", cc);
    let network = network_start(node_config.clone(), cc).await?;
    let node = Node::start(Box::new(storage), node_config, network.broker).await?;
    Fledger::run(node, args).await
}

impl Fledger {
    async fn run(node: Node, args: Args) -> anyhow::Result<()> {
        let nc = &node.node_config.info;
        log::info!("Started node {}: {}", nc.get_id(), nc.name);

        let mut f = Fledger {
            ds: node.dht_storage.as_ref().unwrap().clone(),
            node,
            args,
        };

        match f.args.command.clone() {
            Some(cmd) => match cmd {
                Commands::Realm { command } => RealmHandler::run(f, command).await,
                Commands::Crypto {} => todo!(),
                Commands::Stats {} => todo!(),
                Commands::Page { command } => Page::run(f, command).await,
            },
            None => f.loop_node(None).await,
        }
    }

    pub async fn loop_node(&mut self, max_count: Option<u32>) -> anyhow::Result<()> {
        let mut i: u32 = 0;
        self.node
            .broker_net
            .emit_msg_in(NetworkIn::WSUpdateListRequest)?;
        loop {
            i += 1;

            wait_ms(1000).await;

            self.log(i).await?;
            self.ds.sync()?;
            let dr = self.node.dht_router.as_ref().unwrap().stats.borrow();
            println!("nodes: {}/{:?}", dr.active, dr.all_nodes);

            if let Some(c) = max_count.as_ref() {
                if c < &i {
                    return Ok(());
                }
            }
        }
    }

    pub async fn log(&mut self, i: u32) -> anyhow::Result<()> {
        if i % self.args.log_freq == self.args.log_freq - 1 {
            if self.args.log_random {
                log::info!(
                    "Nodes are: {}",
                    self.node
                        .nodes_online()?
                        .iter()
                        .map(|n| format!("{}/{}", n.name, n.get_id()))
                        .collect::<Vec<_>>()
                        .join(" - ")
                );
            }

            if self.args.log_countdown {
                let ping = &self.node.ping.as_ref().unwrap().storage.borrow();
                log::info!("Nodes countdowns are: {:?}", ping.stats);
            }

            if self.args.log_gossip {
                log::debug!(
                    "Chat messages are: {:?}",
                    self.node.gossip.as_ref().unwrap().chat_events()
                );
            }

            if self.args.log_dht_storage {
                if let Some(ds) = self.node.dht_storage.as_mut() {
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
