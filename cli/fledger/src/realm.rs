use clap::Subcommand;
use flcrypto::{access::Condition, signer::SignerTrait};
use flmodules::{
    dht_storage::{core::RealmConfig, realm_view::RealmViewBuilder},
    flo::realm::Realm,
};

use crate::{Fledger, FledgerState};

#[derive(Subcommand, Debug, Clone)]
pub enum RealmCommands {
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
        /// Sets the condition to Condition::Pass - useful for testing
        #[clap(long, default_value_t = false)]
        cond_pass: bool,
    },
}

pub struct RealmHandler {}

impl RealmHandler {
    pub async fn run(f: Fledger, command: RealmCommands) -> anyhow::Result<()> {
        match command {
            RealmCommands::List => Self::list_realms(f).await,
            RealmCommands::Create {
                name,
                max_space,
                max_flo_size,
                cond_pass,
            } => Self::realm_create(f, name.clone(), max_space, max_flo_size, cond_pass).await,
        }
    }

    async fn realm_create(
        mut f: Fledger,
        name: String,
        max_space: Option<u64>,
        max_flo_size: Option<u32>,
        cond_pass: bool,
    ) -> anyhow::Result<()> {
        f.loop_node(crate::FledgerState::Connected(1)).await?;

        let config = RealmConfig {
            max_space: max_space.unwrap_or(1000000),
            max_flo_size: max_flo_size.unwrap_or(10000),
        };
        log::info!(
            "Creating realm with name '{name}' / total space: {} / flo size: {}",
            config.max_space,
            config.max_flo_size
        );
        let signer = f.node.crypto_storage.get_signer();
        let cond = cond_pass
            .then(|| Condition::Pass)
            .unwrap_or(Condition::Verifier(signer.verifier()));
        let signers = vec![signer];
        RealmViewBuilder::new(f.ds.clone(), name, cond.clone(), signers.clone())
            .config(config)
            .root_http(
                "danu".to_string(),
                INDEX_HTML.to_string(),
                None,
                cond.clone(),
                signers.clone(),
            )
            .root_tag("danu".to_string(), None, cond.clone(), signers)
            .build()
            .await?;

        f.loop_node(FledgerState::Sync(3)).await?;

        Self::list_realms(f).await
    }

    async fn list_realms(mut f: Fledger) -> anyhow::Result<()> {
        f.loop_node(FledgerState::Sync(5)).await?;
        let rids = f.ds.get_realm_ids().await?;
        if rids.len() == 0 {
            println!("No realms found.");
            return Ok(());
        }
        if f.args.verbosity.log_level_filter() != log::LevelFilter::Off {
            for rid in rids.iter() {
                if let Ok(realm) = f.ds.get_flo::<Realm>(&rid.into()).await {
                    println!("\nRealm: '{}'", realm.cache().get_name(),);
                    println!("  ID: {:?}", realm.flo_id());
                    println!("  Config: {:?}", realm.cache().get_config());
                    for (k, v) in realm.cache().get_services() {
                        println!("  Service '{}' - {:?}", k, v);
                    }
                }
            }
        } else {
            println!(
                "Realm-IDs are: {}",
                rids.iter()
                    .map(|rid| format!("{rid}"))
                    .collect::<Vec<_>>()
                    .join(" :: ")
            );
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
