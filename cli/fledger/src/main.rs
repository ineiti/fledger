use clap::{Parser, Subcommand};

use flarch::{
    data_storage::{DataStorage, DataStorageFile},
    nodeids::NodeID,
    tasks::wait_ms,
    web_rtc::connection::{ConnectionConfig, HostLogin},
};
use flmodules::{
    dht_router::broker::DHTRouter,
    dht_storage::broker::DHTStorage,
    flo::realm::RealmID,
    network::{broker::NetworkIn, signal::SIGNAL_VERSION},
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

    /// Update frequency of dht_storage in seconds
    #[arg(long, default_value = "60")]
    dht_storage_freq: u32,

    /// Log gossip messages
    #[arg(long, default_value = "false")]
    log_gossip: bool,

    /// Log random router
    #[arg(long, default_value = "false")]
    log_random: bool,

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
    dr: DHTRouter,
    args: Args,
    realm_ids: Option<Vec<RealmID>>,
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
    let node = Node::start_network(Box::new(storage), node_config, cc).await?;
    Fledger::run(node, args).await
}

pub enum FledgerState {
    Connected(usize),
    DHTAvailable,
    Sync(usize),
    Duration(usize),
    Forever,
}

impl Fledger {
    async fn run(node: Node, args: Args) -> anyhow::Result<()> {
        let nc = &node.node_config.info;
        log::info!("Started node {}: {}", nc.get_id(), nc.name);

        let mut f = Fledger {
            ds: node.dht_storage.as_ref().unwrap().clone(),
            dr: node.dht_router.as_ref().unwrap().clone(),
            node,
            args,
            realm_ids: None,
        };

        match f.args.command.clone() {
            Some(cmd) => match cmd {
                Commands::Realm { command } => RealmHandler::run(f, command).await,
                Commands::Crypto {} => todo!(),
                Commands::Stats {} => todo!(),
                Commands::Page { command } => Page::run(f, command).await,
            },
            None => f.loop_node(FledgerState::Forever).await,
        }
    }

    pub async fn loop_node(&mut self, state: FledgerState) -> anyhow::Result<()> {
        let mut count: usize = 0;
        self.node
            .broker_net
            .emit_msg_in(NetworkIn::WSUpdateListRequest)?;
        match state {
            FledgerState::Connected(i) => log::info!("Waiting for {i} connected nodes"),
            FledgerState::DHTAvailable => log::info!("Waiting for a DHT to be available"),
            FledgerState::Sync(_) => log::info!("Synching with neighbours"),
            FledgerState::Duration(i) => log::info!("Just hanging around {i} seconds"),
            FledgerState::Forever => log::info!("Looping forever"),
        }

        let mut stats = self.dr.stats.borrow().clone();
        loop {
            count += 1;

            wait_ms(1000).await;

            if match state {
                FledgerState::Connected(i) => self.dr.stats.borrow().active >= i,
                FledgerState::DHTAvailable => !self.ds.stats.borrow().realm_stats.is_empty(),
                FledgerState::Sync(i) => {
                    if self.dr.stats.borrow().active == 0
                        || self.ds.stats.borrow().realm_stats.is_empty()
                    {
                        count = 0;
                    } else {
                        self.ds.sync()?;
                        self.ds.propagate()?;
                    }
                    count > i
                }
                FledgerState::Duration(i) => count >= i,
                FledgerState::Forever => {
                    self.log(count as u32).await?;
                    false
                }
            } {
                return Ok(());
            }
            if count as u32 % self.args.dht_storage_freq == 0 {
                self.ds.sync()?;
            }
            let stats_new = self.dr.stats.borrow().clone();
            if stats != stats_new {
                stats = stats_new;
                let active = stats.bucket_nodes.iter().flatten().collect::<Vec<_>>();
                let rest = stats
                    .all_nodes
                    .iter()
                    .filter(|id| !active.contains(id))
                    .collect::<Vec<_>>();
                println!(
                    "dht-connections:   active({}): {}\n                 inactive({}): {}",
                    stats.active,
                    self.print_nodes(active),
                    rest.len(),
                    self.print_nodes(rest)
                );
            }
        }
    }

    fn print_nodes(&self, ids: Vec<&NodeID>) -> String {
        let mut ret = vec![];
        for id in ids {
            let mut one = format!("{id}");
            if let Ok(list) = self.node.nodes_info_all() {
                if let Some(info) = list.get(id) {
                    one += &format!("('{}')", info.name);
                }
            }
            ret.push(one);
        }
        ret.join(" - ")
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

            if self.args.log_gossip {
                log::debug!(
                    "Chat messages are: {:?}",
                    self.node.gossip.as_ref().unwrap().chat_events()
                );
            }

            if self.args.log_dht_storage {
                if let Some(ds) = self.node.dht_storage.as_mut() {
                    let rids = ds
                        .stats
                        .borrow()
                        .realm_stats
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>();
                    if rids.len() == 0 {
                        log::info!("No realms found.");
                        ds.get_realm_ids().await?;
                        return Ok(());
                    }
                    let print = if let Some(rids_old) = &self.realm_ids {
                        rids_old != &rids
                    } else {
                        true
                    };
                    if print {
                        log::info!(
                            "Realm-IDs are: {}",
                            rids.iter()
                                .map(|rid| format!("{rid}"))
                                .collect::<Vec<_>>()
                                .join(" :: ")
                        );
                        self.realm_ids = Some(rids);
                    }
                };
            }
        }

        Ok(())
    }
}
