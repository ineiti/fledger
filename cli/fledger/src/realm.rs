use clap::Subcommand;
use flcrypto::{access::Condition, signer::SignerTrait};
use flmodules::{
    dht_storage::{core::RealmConfig, realm_view::RealmView},
    flo::{crypto::FloVerifier, realm::Realm},
};

use crate::Fledger;

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
            } => Self::realm_create(f, name.clone(), max_space, max_flo_size).await,
        }
    }

    async fn realm_create(
        mut f: Fledger,
        name: String,
        max_space: Option<u64>,
        max_flo_size: Option<u32>,
    ) -> anyhow::Result<()> {
        log::info!("Waiting for connection to other nodes");
        f.loop_node(Some(2)).await?;

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
        let signers = &[&signer];
        let cond = Condition::Verifier(signer.verifier());
        let mut rv =
            RealmView::new_create_realm_config(f.ds.clone(), &name, cond.clone(), config, signers)
                .await?;
        f.ds.store_flo(FloVerifier::new(rv.realm.realm_id(), signer.verifier()).into())?;
        let root_http = rv
            .create_http(
                "fledger",
                INDEX_HTML.to_string(),
                None,
                cond.clone(),
                signers,
            )
            .await?;
        rv.set_realm_http(root_http.blob_id(), signers).await?;
        let root_tag = rv.create_tag("fledger", None, cond.clone(), signers)?;
        rv.set_realm_tag(root_tag.blob_id(), signers).await?;

        log::info!("Waiting for propagation");
        f.ds.propagate()?;
        f.loop_node(Some(2)).await?;

        Self::list_realms(f).await
    }

    async fn list_realms(mut f: Fledger) -> anyhow::Result<()> {
        log::info!("Waiting for update of data");
        f.loop_node(Some(5)).await?;
        log::info!("Requesting sync of DHT-storage and waiting for answers");
        f.ds.sync()?;
        f.ds.propagate()?;
        f.loop_node(Some(10)).await?;
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
