use std::collections::HashMap;

use crate::Fledger;
use anyhow::anyhow;
use clap::Subcommand;
use flcrypto::{
    access::Condition,
    signer::{SignerTrait, VerifierTrait},
};
use flmodules::{
    dht_storage::{broker::DHTStorage, core::Cuckoo, realm_view::RealmView},
    flo::{blob::BlobAccess, realm::RealmID},
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
        /// The path of the new page.
        /// The path must be accessible to your badge.
        path: String,
        /// The realm of the new page.
        realm: String,
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
    pub async fn run(f: Fledger, cmd: PageCommands) -> anyhow::Result<()> {
        let mut ds = f.node.dht_storage.as_ref().unwrap().clone();
        let mut realms = HashMap::new();
        for rid in ds.get_realm_ids().await? {
            realms.insert(rid.clone(), ds.get_realm_view(rid).await?);
        }
        let mut page = Page { f, ds, realms };

        match cmd {
            PageCommands::List => {
                page.page_list().await?;
                return Ok(());
            }
            PageCommands::Create {
                path,
                realm,
                content,
            } => page.page_create(path, realm, content).await,
            PageCommands::Modify {
                realm,
                path,
                command,
            } => page.page_modify(realm, path, command).await,
        }?;
        page.f.loop_node(Some(3)).await?;
        log::info!("Requesting propagation");
        page.ds.propagate()?;
        page.f.loop_node(Some(3)).await?;
        Ok(())
    }

    async fn page_list(&mut self) -> anyhow::Result<()> {
        for (rid, rv) in &self.realms {
            log::info!("\nRealm: {}", rid);
            let vid = self.f.node.crypto_storage.get_signer().verifier().get_id();
            for (_, page) in rv.pages.as_ref().unwrap().storage.iter() {
                log::info!(
                    "{page}\n    editable: {}",
                    self.ds
                        .convert(page.cond(), &page.realm_id())
                        .await
                        .can_verify(&[&vid])
                );
            }
        }
        Ok(())
    }

    async fn page_create(
        &mut self,
        path: String,
        realm: String,
        content: String,
    ) -> anyhow::Result<()> {
        let mut parts = path.split("/").collect::<Vec<_>>();
        if let Some(new_path) = parts.pop() {
            if parts.is_empty() {
                return Err(anyhow!("Cannot work with empty path"));
            }
            let signer = self.f.node.crypto_storage.get_signer();
            let mut rv = self.get_rv(&realm)?;
            let parent_id = if parts.is_empty() {
                None
            } else {
                let parent_path = parts.join("/");
                let parent = rv.get_page_path(&parent_path)?;
                Some(parent.blob_id())
            };
            let cuckoo = if parts.is_empty() {
                Cuckoo::Parent((*rv.pages.as_ref().unwrap().root).into())
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
        Ok(())
    }

    async fn page_modify(
        &mut self,
        realm: String,
        path: String,
        command: ModifyCommands,
    ) -> anyhow::Result<()> {
        let rv = self.get_rv(&realm)?;
        let page = rv.get_page_path(&path)?;
        let signer = self.f.node.crypto_storage.get_signer();
        let cond = self.ds.convert(page.cond(), &rv.realm.realm_id()).await;
        if !cond.can_verify(&[&signer.get_id()]) {
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
