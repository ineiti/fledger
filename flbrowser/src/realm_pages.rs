use std::collections::HashMap;

use flarch::tasks::{spawn_local, wait_ms};
use flmodules::{
    dht_storage::{
        broker::DHTStorage,
        realm_view::{RealmStorage, RealmView},
    },
    flo::{
        blob::{BlobPage, BlobTag},
        realm::{FloRealm, GlobalID, RealmID},
    },
};
use tokio::sync::{mpsc, watch};

#[derive(Debug, Clone)]
pub struct PageFetcher {
    pub page_tags_rx: watch::Receiver<HashMap<RealmID, PageTags>>,
    pub _request_tx: mpsc::Sender<GlobalID>,
}

struct BackgroundFetcher {
    page_tags_tx: watch::Sender<HashMap<RealmID, PageTags>>,
    request_rx: mpsc::Receiver<GlobalID>,
    realm_views: HashMap<RealmID, RealmView>,
    dht_storage: DHTStorage,
}

#[derive(Debug, Clone)]
pub struct PageTags {
    pub realm: FloRealm,
    pub pages: Option<RealmStorage<BlobPage>>,
    pub _tags: Option<RealmStorage<BlobTag>>,
}

impl PageFetcher {
    pub async fn new(dht_storage: DHTStorage) -> Self {
        let (page_tags_tx, page_tags_rx) = watch::channel(HashMap::new());
        let (_request_tx, request_rx) = mpsc::channel(100);
        spawn_local(async move {
            BackgroundFetcher {
                page_tags_tx,
                request_rx,
                dht_storage,
                realm_views: HashMap::new(),
            }
            .run()
            .await;
        });
        Self {
            page_tags_rx,
            _request_tx,
        }
    }
}

impl BackgroundFetcher {
    async fn run(&mut self) {
        let mut counter = 0;
        loop {
            counter += 1;
            if let Err(e) = self.update(counter).await {
                log::error!("While updating pages: {e:?}");
            }
            wait_ms(1000).await;
        }
    }

    async fn update(&mut self, counter: i32) -> anyhow::Result<()> {
        if counter % 5 == 4 {
            if let Ok(realms) = self.dht_storage.get_realm_ids().await {
                self.update_realms(realms).await?;
            }
            self.update_pages().await?;
        }
        while let Ok(msg) = self.request_rx.try_recv() {
            if let Some(rv) = self.realm_views.get_mut(msg.realm_id()) {
                rv.read_pages((**msg.flo_id()).into()).await?
            }
        }
        self.page_tags_tx.send(
            self.realm_views
                .iter()
                .map(|(rid, rv)| {
                    (
                        rid.clone(),
                        PageTags {
                            realm: rv.realm.clone(),
                            pages: rv.pages.clone(),
                            _tags: rv.tags.clone(),
                        },
                    )
                })
                .collect(),
        )?;

        Ok(())
    }

    async fn update_realms(&mut self, realms: Vec<RealmID>) -> anyhow::Result<()> {
        for realm in realms {
            if !self.realm_views.contains_key(&realm) {
                if let Ok(rv) =
                    RealmView::new_from_id(self.dht_storage.clone(), realm.clone()).await
                {
                    self.realm_views.insert(realm, rv);
                }
            }
        }

        Ok(())
    }

    async fn update_pages(&mut self) -> anyhow::Result<()> {
        for (_, rv) in &mut self.realm_views {
            rv.update_pages().await?;
        }
        Ok(())
    }
}
