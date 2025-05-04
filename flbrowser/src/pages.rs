use flarch::tasks::spawn_local_nosend;
use flcrypto::{access::Condition, signer::KeyPairID};
use flmodules::{
    dht_storage::{broker::DHTStorageOut, core::Cuckoo, realm_view::RealmView},
    flo::{
        blob::{BlobID, BlobPath, FloBlobPage},
        flo::FloID,
        realm::RealmID,
    },
};
use tokio::sync::broadcast;

use crate::web::{getEditorContent, setEditorContent, Button, Web};

#[derive(Debug)]
pub struct DhtPage {
    pub realm: RealmID,
    pub page: FloBlobPage,
    pub path: String,
}

impl From<FloBlobPage> for DhtPage {
    fn from(value: FloBlobPage) -> Self {
        DhtPage {
            realm: value.realm_id(),
            // TODO: this will not work for paths with multiple elements
            path: value.get_path().unwrap_or(&"".to_string()).into(),
            page: value,
        }
    }
}

pub struct Pages {
    rv: RealmView,
    web: Web,
}

impl Pages {
    pub async fn new(mut rv: RealmView, mut rx: broadcast::Receiver<Button>) -> anyhow::Result<()> {
        let mut tap = rv.dht_storage.broker.get_tap_out().await?.0;
        let mut p = Pages {
            rv,
            web: Web::new()?,
        };

        p.show_page(&p.get_dht_page(&p.rv.pages.root).unwrap());
        p.init_editor().await;

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

    async fn clicked(&mut self, btn: Button) {
        match btn {
            Button::CreatePage => {
                let page_path = self.web.get_input("page-path").value();
                let page_content: String = getEditorContent().into();

                log::info!("Storing page: {page_path} - {page_content}");

                let (parent, path) = match self.rv.get_new_page_path(&page_path) {
                    Ok(pp) => pp,
                    Err(e) => {
                        log::error!("While splitting path: {e:?}");
                        return;
                    }
                };
                let cuckoo = Cuckoo::Parent(parent.flo_id());
                match self
                    .rv
                    .create_http_cuckoo(&path, page_content, None, Condition::Pass, cuckoo, &[])
                    .await
                {
                    Ok(page) => self.edit_page(&page.flo_id()),
                    Err(e) => log::error!("While saving page: {e:?}"),
                }
            }
            Button::UpdatePage => {}
            Button::ResetPage => self.reset_page(),
            Button::EditPage(id) => {
                self.edit_page(&id);
            }
            Button::ViewPage(id) => {
                if let Some(page) = self.get_dht_page(&(*id).into()) {
                    self.show_page(&page);
                }
            }
            _ => {}
        }
    }

    async fn dht_msg(&mut self, msg: DHTStorageOut) {
        match msg {
            DHTStorageOut::FloValue(_) => log::trace!("Got new value"),
            DHTStorageOut::CuckooIDs(_gid, _fids) => log::trace!("Got new cuckoos"),
            _ => {}
        }
    }
    fn show_page(&mut self, dp: &DhtPage) {
        self.web.set_id_inner("dht_page", &dp.page.get_index());
        self.web.set_id_inner(
            "dht_page_path",
            &format!("{}/{}", dp.realm, dp.path.clone()),
        );
    }

    async fn init_editor(&mut self) {
        let mut our_pages = vec![];
        for (_, fp) in &self.rv.pages.storage {
            if self
                .rv
                .dht_storage
                .convert(fp.cond(), &fp.realm_id())
                .await
                .can_verify(&[&KeyPairID::rnd()])
            {
                our_pages.push(fp.clone());
            }
        }
        self.reset_page();

        self.web.set_editable_pages(&our_pages);
    }

    fn reset_page(&mut self) {
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

        let path = format!(
            "/{}/{}",
            self.get_dht_page(&self.rv.pages.root).unwrap().path,
            names::Generator::default().next().unwrap()
        );
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

    fn get_dht_page(&self, id: &BlobID) -> Option<DhtPage> {
        self.rv
            .pages
            .storage
            .get(&(**id).into())
            .map(|root_page| root_page.clone().into())
    }

    fn edit_page(&mut self, id: &FloID) {
        if let Some(dp) = self.get_dht_page(&(**id).into()) {
            setEditorContent(dp.page.get_index().into());
            self.set_editor_id_buttons(Some(dp.page.flo_id()), false, true, true);
            self.web.get_input("page-path").set_value(&dp.path);
            return;
        }
        log::error!("Didn't find page with id {id}");
    }
}
