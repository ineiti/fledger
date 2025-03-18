use clap::{Parser, Subcommand};

use flarch::{
    data_storage::{DataStorage, DataStorageFile},
    tasks::wait_ms,
    web_rtc::connection::{ConnectionConfig, HostLogin},
};
use flcrypto::{access::Condition, signer::SignerTrait};
use flmodules::{
    dht_storage::{broker::DHTStorage, core::RealmConfig, realm_view::RealmView},
    flo::crypto::FloVerifier,
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
    Ok(Fledger {
        ds: node.dht_storage.as_ref().unwrap().clone(),
        node,
        args,
    }
    .run()
    .await?)
}

impl Fledger {
    async fn run(&mut self) -> anyhow::Result<()> {
        let nc = &self.node.node_config.info;
        log::info!("Started node {}: {}", nc.get_id(), nc.name);

        match &self.args.command {
            Some(cmd) => match cmd {
                Commands::Realm { command } => match command {
                    RealmCommands::List => self.list_realms().await?,
                    RealmCommands::Create {
                        name,
                        max_space,
                        max_flo_size,
                    } => {
                        self.realm_create(name.clone(), *max_space, *max_flo_size)
                            .await?
                    }
                },
                Commands::Crypto {} => todo!(),
                Commands::Stats {} => todo!(),
            },
            None => self.loop_node(None).await?,
        }

        Ok(())
    }

    async fn realm_create(
        &mut self,
        name: String,
        max_space: Option<u64>,
        max_flo_size: Option<u32>,
    ) -> anyhow::Result<()> {
        log::info!("Waiting for connection to other nodes");
        self.loop_node(Some(2)).await?;

        let config = RealmConfig {
            max_space: max_space.unwrap_or(1000000),
            max_flo_size: max_flo_size.unwrap_or(10000),
        };
        log::info!(
            "Creating realm with name '{name}' / total space: {} / flo size: {}",
            config.max_space,
            config.max_flo_size
        );
        let signer = self.node.crypto_storage.get_signer();
        let signers = &[&signer];
        let cond = Condition::Verifier(signer.verifier());
        let mut rv = RealmView::new_create_realm_config(
            self.ds.clone(),
            &name,
            cond.clone(),
            config,
            signers,
        )
        .await?;
        self.ds
            .store_flo(FloVerifier::new(rv.realm.realm_id(), signer.verifier()).into())?;
        let root_http = rv.create_http(
            "fledger",
            INDEX_HTML.to_string(),
            None,
            cond.clone(),
            signers,
        )?;
        rv.set_realm_http(root_http.blob_id(), signers).await?;
        let root_tag = rv.create_tag("fledger", None, cond.clone(), signers)?;
        rv.set_realm_tag(root_tag.blob_id(), signers).await?;

        log::info!("Waiting for propagation");
        self.ds.propagate()?;
        self.loop_node(Some(2)).await?;

        self.list_realms().await?;

        Ok(())
    }

    async fn list_realms(&mut self) -> anyhow::Result<()> {
        log::info!("Waiting for update of data");
        self.loop_node(Some(20)).await?;
        log::info!("Requesting sync of DHT-storage and waiting for answers");
        self.ds.sync()?;
        self.ds.propagate()?;
        self.loop_node(Some(2)).await?;
        let rids = self.ds.get_realm_ids().await?;
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

    async fn loop_node(&mut self, max_count: Option<u32>) -> anyhow::Result<()> {
        let mut i: u32 = 0;
        self.node
            .broker_net
            .emit_msg_in(NetworkIn::WSUpdateListRequest)?;
        loop {
            i += 1;

            wait_ms(1000).await;

            self.log(i).await?;

            if let Some(c) = max_count.as_ref() {
                if c < &i {
                    return Ok(());
                }
            }
        }
    }

    async fn log(&mut self, i: u32) -> anyhow::Result<()> {
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

const INDEX_HTML: &str = r##"
<!DOCTYPE html>
<html>
  <head>
    <title>Fledger</title>
  </head>
<body>
<h1>Fledger</h1>

Fast, Fun, Fair Ledger, or Fledger puts the <strong>FUN</strong> back in blockchain!
</body>
</html>
    "##;
