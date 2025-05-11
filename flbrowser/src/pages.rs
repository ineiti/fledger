use flarch::tasks::spawn_local_nosend;
use flcrypto::{
    access::Condition,
    signer::{Signer, SignerTrait, Verifier},
};
use flmodules::{
    dht_storage::{broker::DHTStorageOut, core::Cuckoo, realm_view::RealmView},
    flo::{
        blob::{BlobAccess, BlobFamily, BlobID, BlobPath, FloBlobPage},
        flo::FloID,
    },
};
use tokio::sync::broadcast;

use crate::web::{getEditorContent, setEditorContent, Button, Tab, Web};

pub struct Pages {
    rv: RealmView,
    web: Web,
    edit_id: Option<BlobID>,
    home_id: BlobID,
    verifier: Verifier,
    signers: Vec<Signer>,
}

impl Pages {
    pub async fn new(
        mut rv: RealmView,
        mut rx: broadcast::Receiver<Button>,
        signer: Signer,
    ) -> anyhow::Result<()> {
        let mut tap = rv.get_tap_out().await?.0;
        let web = Web::new()?;
        let page = Self::get_home_page(&rv, &web)?;
        let mut p = Pages {
            home_id: page.blob_id(),
            rv,
            web,
            edit_id: None,
            verifier: signer.verifier(),
            signers: vec![signer],
        };

        p.show_home_page(&page).await;
        p.reset_page();
        p.set_editable_pages().await;

        spawn_local_nosend(async move {
            loop {
                tokio::select! {
                    Some(msg) = tap.recv() => {p.dht_msg(msg).await;}
                    Ok(btn) = rx.recv() => {p.clicked(btn).await;}
                }
            }
        });

        Ok(())
    }

    fn get_home_page(rv: &RealmView, web: &Web) -> anyhow::Result<FloBlobPage> {
        if let Some(hash) = web.get_hash() {
            if hash.starts_with("#web") {
                if let Ok(page) = rv
                    .get_page_from_path(&hash.trim_start_matches("#web"))
                    .cloned()
                {
                    return Ok(page);
                }
            }
        }
        rv.pages
            .storage
            .get(&rv.pages.root)
            .cloned()
            .ok_or(anyhow::anyhow!("Didn't find page"))
    }

    async fn clicked(&mut self, btn: Button) {
        match btn {
            Button::CreatePage => self.create_page().await,
            Button::UpdatePage => self.store_edit_page().await.expect("Updating the page"),
            Button::ResetPage => self.reset_page(),
            Button::EditPage(id) => {
                self.edit_page(&id);
            }
            Button::ViewPage(id) => {
                if let Some(page) = self.get_page(&(*id).into()) {
                    self.show_home_page(&page).await;
                }
            }
            Button::DebugPage(id) => {
                if let Some(page) = self.get_page(&(*id).into()) {
                    log::info!("Page is: {page:?}");
                }
            }
            Button::AnchorPage(path) => {
                match self.rv.get_page_from_path(&path).cloned() {
                    Ok(page) => self.show_home_page(&page).await,
                    Err(e) => log::warn!("Couldn't visit {path}: {e:?}"),
                }
            }
            _ => {}
        }
    }

    // Handles various cases when creating a page:
    // - path: prepend a "/" to search for an existing path with a parent.
    //   If no parent where we can attach is found, only keep the last element of the path.
    //   Remove an eventual starting "/" when storing a single page, and replace all other "/" with a "_".
    // - parent: if it finds a parent which is modifiable with self.signer, attach to this parent,
    //   and don't attach as a cuckoo.
    //   If it doesn't find a parent, or if the parent is not modifiable with self.signer, convert
    //   the path by replacing "/" with "_".
    async fn create_page(&mut self) {
        let (parent, path) = self
            .path_to_parent(&self.web.get_input("page-path").value())
            .await;
        let page_content: String = getEditorContent().into();

        let signers = self.signers.iter().collect::<Vec<_>>();
        match self
            .rv
            .create_http_cuckoo(
                &path,
                page_content,
                parent.map(|fp| fp.blob_id()),
                Condition::Verifier(self.verifier.clone()),
                Cuckoo::Parent((*self.rv.pages.root.clone()).into()),
                &signers,
            )
            .await
        {
            Ok(page) => self.edit_page(&page.flo_id()),
            Err(e) => log::error!("While saving page: {e:?}"),
        }
        self.rv.update_pages().await.expect("Updating pages");
        self.set_editable_pages().await;
    }

    async fn path_to_parent(&mut self, path_raw: &str) -> (Option<FloBlobPage>, String) {
        let path = format!("/{path_raw}")
            .trim()
            .replace("//", "/")
            .trim_end_matches('/')
            .to_string();

        if let Ok(pp) = self.rv.get_page_parent_remaining(&path) {
            let cond = self.rv.get_cond(&pp.0).await;
            if cond.can_signers(&self.signers.iter().collect::<Vec<_>>()) {
                return (Some(pp.0), pp.1);
            }
            log::warn!(
                "Couldn't attach to parent {} because our key cannot sign",
                pp.0
            );
            return (None, pp.1);
        }

        return (None, path.trim_start_matches("/").replace("/", "_"));
    }

    async fn store_edit_page(&mut self) -> anyhow::Result<()> {
        let id = self
            .edit_id
            .as_ref()
            .cloned()
            .ok_or(anyhow::anyhow!("No ID stored for current page"))?;
        let content = getEditorContent()
            .as_string()
            .ok_or(anyhow::anyhow!("Couldn't convert content"))?;
        let (parent, path) = self
            .path_to_parent(&self.web.get_input("page-path").value())
            .await;

        let signers = self.signers.iter().collect::<Vec<_>>();
        self.rv
            .update_page(
                &id,
                |bp| {
                    bp.set_data("index.html".into(), content.into());
                    bp.set_path(path);
                    bp.set_parents(parent.map(|p| vec![p.blob_id()]).unwrap_or(vec![]));
                },
                &signers,
            )
            .await?;
        self.set_editable_pages().await;
        if self.home_id == id {
            self.update_home_page().await;
        }

        Ok(())
    }

    async fn dht_msg(&mut self, msg: DHTStorageOut) {
        match msg {
            DHTStorageOut::FloValue(_) => log::trace!("Got new value"),
            DHTStorageOut::CuckooIDs(_gid, _fids) => log::trace!("Got new cuckoos"),
            _ => {}
        }
    }

    async fn show_home_page(&mut self, fp: &FloBlobPage) {
        self.home_id = fp.blob_id();
        self.update_home_page().await;
        let full_path = format!(
            "#web{}",
            self.rv
                .get_full_path_blob(fp)
                .unwrap_or("Unknown".to_string())
        );
        if self.web.get_hash().unwrap_or("".to_string()) != full_path {
            self.web.set_hash(&full_path);
        }
        self.web.set_tab(Tab::Home);
    }

    async fn update_home_page(&mut self) {
        let home_page = self
            .rv
            .pages
            .storage
            .get(&self.home_id)
            .expect("Getting home page")
            .clone();
        self.web.set_id_inner("dht_page", &home_page.get_index());
        self.web.set_id_inner(
            "dht_page_path",
            &format!(
                "{}{}",
                home_page.realm_id(),
                self.rv
                    .get_full_path_blob(&home_page)
                    .unwrap_or("Unknown".to_string())
            ),
        );

        let parent = match &home_page.flo().flo_config().cuckoo {
            Cuckoo::Parent(p) => self.rv.pages.storage.get(&(**p).into()).cloned(),
            _ => None,
        };
        let attached = self
            .rv
            .pages
            .get_cuckoos(home_page.blob_id())
            .await
            .expect("getting cuckoos")
            .into_iter()
            .filter(|fp| home_page.flo_id() != fp.flo_id())
            .collect::<Vec<_>>();
        self.web.page_cuckoos(parent.as_ref(), &attached);
        let parents = home_page
            .get_parents()
            .iter()
            .filter_map(|id| self.rv.pages.storage.get(id))
            .collect::<Vec<_>>();
        let children = home_page
            .get_children()
            .iter()
            .filter_map(|id| self.rv.pages.storage.get(id))
            .collect::<Vec<_>>();
        self.web.page_family(&parents, &children);
    }

    async fn set_editable_pages(&mut self) {
        let mut our_pages = vec![];
        for (_, fp) in &self.rv.pages.storage.clone() {
            let cond = self.rv.get_cond(&fp).await;
            if cond.can_signers(&self.signers.iter().collect::<Vec<_>>()) {
                our_pages.push((
                    self.rv
                        .get_full_path_blob(fp)
                        .unwrap_or("Unknown".to_string()),
                    fp.flo_id(),
                ));
            }
        }
        self.web.set_editable_pages(&our_pages);
    }

    fn reset_page(&mut self) {
        self.edit_id = None;
        setEditorContent(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Hello Danu</title>
</head>
<body>
  <h1>Hello Danu</h1>
  <p>Welcome to Danu.</p>
</body>
</html>"#
                .into(),
        );

        let path = format!("/{}", names::Generator::default().next().unwrap());
        self.web.get_input("page-path").set_value(&path);

        self.set_editor_id_buttons(None, true, false, false);
    }

    fn set_editor_id_buttons(
        &mut self,
        id: Option<FloID>,
        create: bool,
        update: bool,
        reset: bool,
    ) {
        self.web.set_visible("create-page", create);
        self.web.set_visible("update-page", update);
        self.web.set_visible("reset-page", reset);
        self.web.set_visible("page-id-div", id.is_some());
        if let Some(id) = id {
            self.web
                .get_el("page-id")
                .set_inner_html(&format!("{id:x}"));
        }
    }

    fn get_page(&self, id: &BlobID) -> Option<FloBlobPage> {
        self.rv.pages.storage.get(&(**id).into()).cloned()
    }

    fn edit_page(&mut self, id: &FloID) {
        self.edit_id = Some((*id.clone()).into());
        if let Some(dp) = self.get_page(&(**id).into()) {
            setEditorContent(dp.get_index().into());
            self.set_editor_id_buttons(Some(dp.flo_id()), false, true, true);
            let path = self
                .rv
                .get_full_path_blob(&dp)
                .unwrap_or(dp.get_path().unwrap_or(&"Unknown".to_string()).to_string());
            self.web.get_input("page-path").set_value(&path);
            return;
        }
        log::error!("Didn't find page with id {id}");
    }
}
