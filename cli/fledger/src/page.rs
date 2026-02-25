use std::collections::HashMap;

use crate::{Fledger, FledgerState};
use anyhow::anyhow;
use clap::Subcommand;
use flcrypto::{
    access::Condition,
    signer::{SignerTrait, VerifierTrait},
};
use flmodules::{
    dht_storage::{broker::DHTStorage, core::Cuckoo},
    flo::{blob::BlobAccess, realm::RealmID, realm_view::RealmView},
};

pub struct Page {
    f: Fledger,
    ds: DHTStorage,
    realms: HashMap<RealmID, RealmView>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum PageCommands {
    /// List available pages
    List,
    /// Creates a new page
    Create {
        /// The realm of the new page.
        realm: String,
        /// The path of the new page.
        path: String,
        /// The content of the new page.
        content: String,
    },
    /// Modify a page
    Modify {
        /// The realm of the existing page.
        realm: String,
        /// The path of the page to modify.
        path: String,
        #[command(subcommand)]
        command: ModifyCommands,
    },
    /// Print a page
    Print {
        /// The realm of the existing page.
        realm: String,
        /// The path of the page to modify.
        path: String,
        /// The file to print.
        #[arg(default_value = "index.html")]
        file: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ModifyCommands {
    /// Stores a new content for the index.html
    Content { content: String },
    /// Changes the path
    Path {
        /// The new path for this page - only the last element.
        new_path: String,
    },
}

impl Page {
    pub async fn run(mut f: Fledger, cmd: PageCommands) -> anyhow::Result<()> {
        f.loop_node(FledgerState::DHTAvailable).await?;

        let mut ds = f.node.dht_storage.as_ref().unwrap().clone();
        let mut realms = HashMap::new();
        for rid in ds.get_realm_ids().await? {
            match ds.get_realm_view(rid.clone()).await {
                Ok(realm) => {
                    realms.insert(rid.clone(), realm);
                }
                Err(e) => {
                    log::warn!("Couldn't connect to realm {rid}: {e:?}");
                }
            }
        }
        let mut page = Page { f, ds, realms };

        match cmd {
            PageCommands::List => {
                page.list().await?;
                return Ok(());
            }
            PageCommands::Create {
                path,
                realm,
                content,
            } => page.create(path, realm, content).await,
            PageCommands::Modify {
                realm,
                path,
                command,
            } => page.modify(realm, path, command).await,
            PageCommands::Print { realm, path, file } => page.print(realm, path, file).await,
        }?;
        Ok(())
    }

    async fn list(&mut self) -> anyhow::Result<()> {
        for (rid, rv) in &self.realms {
            log::info!("\nRealm: {}", rid);
            let vid = self.f.node.crypto_storage.get_signer().verifier().get_id();
            for (_, page) in rv.pages.storage.iter() {
                println!(
                    "{page}\n    editable: {}",
                    self.ds
                        .convert(page.cond(), &page.realm_id())
                        .await
                        .can_sign(&[&vid])
                );
            }
        }
        Ok(())
    }

    async fn create(&mut self, path: String, realm: String, content: String) -> anyhow::Result<()> {
        let mut parts = path.split("/").collect::<Vec<_>>();
        if let Some(new_path) = parts.pop() {
            if parts.is_empty() {
                return Err(anyhow!("Cannot work with empty path"));
            }
            self.f.loop_node(FledgerState::Connected(2)).await?;
            let signer = self.f.node.crypto_storage.get_signer();
            let mut rv = self.get_rv(&realm)?;
            let parent_id = if parts.is_empty() {
                None
            } else {
                let parent_path = parts.join("/");
                let parent = rv.get_page_from_path(&parent_path)?;
                Some(parent.blob_id())
            };
            let cuckoo = if parts.is_empty() {
                Cuckoo::Parent((*rv.pages.root).into())
            } else {
                Cuckoo::None
            };
            let new_page = rv
                .create_http_cuckoo(
                    new_path,
                    content,
                    parent_id.clone(),
                    Condition::Verifier(signer.verifier()),
                    cuckoo,
                    &[&signer],
                )
                .await?;
            log::info!("Created new page {new_page}");
        }
        self.f.loop_node(FledgerState::Sync(3)).await?;
        Ok(())
    }

    async fn modify(
        &mut self,
        realm: String,
        path: String,
        command: ModifyCommands,
    ) -> anyhow::Result<()> {
        let rv = self.get_rv(&realm)?;
        let page = rv.get_page_from_path(&path)?;
        let signer = self.f.node.crypto_storage.get_signer();
        self.f.loop_node(FledgerState::Connected(2)).await?;
        let cond = self.ds.convert(page.cond(), &rv.realm.realm_id()).await;
        if !cond.can_sign(&[&signer.get_id()]) {
            return Err(anyhow!("Our signer is not allowed to modify this page!"));
        }
        match command {
            ModifyCommands::Content { mut content } => {
                if std::path::Path::new(&content).exists() {
                    content = std::fs::read_to_string(&content)?;
                }
                let new_page = page.edit_data_signers(
                    cond,
                    |p| {
                        p.0.get_blob_mut()
                            .datas_mut()
                            .insert("index.html".into(), content.into());
                    },
                    &[&signer],
                )?;
                self.ds.store_flo(new_page.into())?;
            }
            ModifyCommands::Path { new_path } => {
                let new_page = page.edit_data_signers(
                    cond,
                    |p| {
                        p.0.get_blob_mut()
                            .values_mut()
                            .insert("path".into(), new_path.into());
                    },
                    &[&signer],
                )?;
                self.ds.store_flo(new_page.into())?;
            }
        }
        self.f.loop_node(FledgerState::Sync(5)).await?;
        Ok(())
    }

    async fn print(&mut self, realm: String, path: String, file: String) -> anyhow::Result<()> {
        let rv = self.get_rv(&realm)?;
        let page = rv.get_page_from_path(&path)?;
        log::info!("Printing {page}");
        if let Some(content) = page.datas().get(&file) {
            println!(
                "{}",
                String::from_utf8(content.to_vec()).unwrap_or_default()
            );
        }
        Ok(())
    }

    fn get_rv(&mut self, realm: &str) -> anyhow::Result<RealmView> {
        let realm_lower = realm.to_ascii_lowercase();
        let rid = self
            .realms
            .keys()
            .find(|id| format!("{id:x}").starts_with(&realm_lower))
            .ok_or_else(|| anyhow!("Didn't find realm starting with {realm}"))?
            .clone();

        // Since we already found the key, we can safely unwrap here
        Ok(self.realms.remove(&rid).unwrap())
    }
}
