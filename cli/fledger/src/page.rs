use std::collections::HashMap;

use crate::Fledger;
use anyhow::anyhow;
use clap::Subcommand;
use flarch::tasks::wait_ms;
use flcrypto::{
    access::Condition,
    signer::{SignerTrait, VerifierTrait},
};
use flmodules::{
    dht_storage::{broker::DHTStorage, core::Cuckoo, realm_view::RealmView},
    flo::realm::RealmID,
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
    /// Deletes a page
    Delete {
        /// The path of the page to delete.
        path: String,
    },
    /// Modify a page
    Modify {
        /// The path of the page to modify.
        path: String,
        /// The new content of the page.
        content: String,
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
            PageCommands::List => page.page_list().await,
            PageCommands::Create {
                path,
                realm,
                content,
            } => page.page_create(path, realm, content).await,
            PageCommands::Delete { path } => page.page_delete(path).await,
            PageCommands::Modify { path, content } => page.page_modify(path, content).await,
        }
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
            let realm_lower = realm.to_ascii_lowercase();
            if let Some(rv) = self
                .realms
                .iter_mut()
                .find(|(id, _)| format!("{id:x}").starts_with(&realm_lower))
                .map(|(_, rv)| rv)
            {
                let parent_id = if parts.is_empty() {
                    None
                } else {
                    let parent_path = parts.join("/");
                    if let Some(parent) = rv.get_page_path(&parent_path) {
                        Some(parent.blob_id())
                    } else {
                        return Err(anyhow!("Didn't find path '{parent_path}'"));
                    }
                };
                let cuckoo = if parts.is_empty() {
                    Cuckoo::Parent((*rv.pages.as_ref().unwrap().root).into())
                } else {
                    Cuckoo::None
                };
                let signer = self.f.node.crypto_storage.get_signer();
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
                self.ds.sync()?;
                wait_ms(1000).await;
                if let Some(pid) = parent_id {
                    rv.update_pages().await?;
                    rv.pages
                        .as_ref()
                        .unwrap()
                        .storage
                        .get(&pid)
                        .map(|parent| log::info!("Parent is: {parent}"));
                }
            } else {
                return Err(anyhow!("Didn't find any realm-id starting with {realm}"));
            }
        }
        Ok(())
    }

    async fn page_delete(&mut self, _path: String) -> anyhow::Result<()> {
        Ok(())
    }

    async fn page_modify(&mut self, _path: String, _content: String) -> anyhow::Result<()> {
        Ok(())
    }
}
