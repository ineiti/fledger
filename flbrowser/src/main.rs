use anyhow::{anyhow, Result};
use chat::Chat;
use flcrypto::signer::SignerTrait;
use flmodules::{
    dht_router::broker::DHTRouter,
    dht_storage::{self, broker::DHTStorage, realm_view::RealmView},
    flo::{crypto::FloVerifier, flo::FloID, realm::RealmID},
    gossip_events::broker::Gossip,
    Modules,
};
use js_sys::JsString;
use pages::Pages;
use proxy::Proxy;
use std::{collections::HashMap, str::FromStr};
use tokio::sync::{broadcast, watch};
use wasm_bindgen::prelude::wasm_bindgen;
use web_sys::{HtmlButtonElement, HtmlElement, HtmlOListElement};

use flarch::{
    data_storage::DataStorageLocal,
    tasks::{spawn_local_nosend, wait_ms},
    web_rtc::connection::{ConnectionConfig, HostLogin},
};
use flmodules::network::network_start;
use flnode::{node::Node, stat::NetStats};

mod web;
use web::*;
mod chat;
mod pages;
mod proxy;

#[derive(Debug)]
struct NetConf<'a> {
    signal_server: &'a str,
    stun_server: Option<&'a str>,
    turn_server: Option<&'a str>,
}

#[cfg(not(feature = "local"))]
const NETWORK_CONFIG: NetConf = NetConf {
    signal_server: "wss://signal.fledg.re",
    stun_server: Some("stun:stun.l.google.com:19302"),
    turn_server: Some("something:something@turn:web.fledg.re:3478"),
};

#[cfg(feature = "local")]
const NETWORK_CONFIG: NetConf = NetConf {
    signal_server: "ws://localhost:8765",
    stun_server: None,
    turn_server: None,
};

pub fn main() {
    console_error_panic_hook::set_once();
    wasm_logger::init(wasm_logger::Config::new(log::Level::Debug));
}

// Because I really want to have a 'normal' HTML file and then link it with rust,
// this code is necessary to link the two.
// Using things like Yew or others is too far from HTML for me.
// Any suggestions for a framework that allows to do this in a cleaner way are welcome.
#[wasm_bindgen]
pub struct JSInterface {
    tx: broadcast::Sender<Button>,
}

#[wasm_bindgen]
impl JSInterface {
    #[wasm_bindgen(constructor)]
    pub async fn new() -> Self {
        Self {
            tx: WebState::start().await.expect("Starting WebState"),
        }
    }

    pub fn visit_page(&mut self, path_js: JsString) {
        let path_hash = path_js.as_string().expect("Convert JsString");
        let path = path_hash.trim_start_matches("#web").to_string();
        self.send_button(Button::AnchorPage(path));
    }

    pub fn button_click(&mut self, btn: JsString) {
        Button::try_from(btn.as_string().expect("JsString to String conversion"))
            .ok()
            .map(|btn| self.send_button(btn));
    }

    pub fn button_page_edit(&mut self, id: JsString) {
        Self::js_to_id(id).map(|fid| self.send_button(Button::EditPage(fid)));
    }

    pub fn button_page_view(&mut self, id: JsString) {
        Self::js_to_id(id).map(|fid| self.send_button(Button::ViewPage(fid)));
    }

    pub fn button_page_debug(&mut self, id: JsString) {
        Self::js_to_id(id).map(|fid| self.send_button(Button::DebugPage(fid)));
    }
    
    fn js_to_id(id: JsString) -> Option<FloID> {
        FloID::from_str(&id.as_string().expect("id to string")).ok()
    }

    fn send_button(&mut self, btn: Button) {
        if let Err(e) = self.tx.send(btn.clone()) {
            log::error!("While sending {btn:?}: {e:?}");
        }
    }
}

#[derive(Debug)]
pub struct WebState {
    webn: WebNode,
    web: Web,
    state: StateEnum,
    status_box: UL,
    values: Option<Values>,
    rx: broadcast::Receiver<Button>,
    tx: broadcast::Sender<Button>,
}

#[derive(Debug)]
enum StateEnum {
    Init,
    ConnectingSignalling,
    ConnectingNodes,
    UpdateDHT,
    ShowPage(RealmView),
    Idle,
}

impl WebState {
    pub async fn start() -> Result<broadcast::Sender<Button>> {
        let (tx, rx) = broadcast::channel(10);
        let txc = tx.clone();
        let mut ws = Self {
            webn: WebNode::new().await?,
            web: Web::new()?,
            state: StateEnum::Init,
            status_box: UL::default(),
            values: None,
            rx,
            tx,
        };

        spawn_local_nosend(async move {
            loop {
                let values = ws.webn.get_values().await;
                ws.web.udpate_values(&values);
                ws.values = Some(values);
                ws.check_state().await;
                tokio::select! {
                    _ = wait_ms(1000) => {},
                    Ok(btn) = ws.rx.recv() => {ws.clicked(btn).await;}
                }
            }
        });

        Ok(txc)
    }

    async fn check_state(&mut self) {
        match self.state {
            StateEnum::Init => self.set_state(StateEnum::ConnectingSignalling).await,
            StateEnum::ConnectingSignalling => {
                if self.webn.stats.online.len() > 0 {
                    self.set_state(StateEnum::ConnectingNodes).await;
                }
            }
            StateEnum::ConnectingNodes => {
                if self.webn.dht_router.stats.borrow().active >= 2 {
                    self.set_state(StateEnum::UpdateDHT).await;
                }
            }
            StateEnum::UpdateDHT => {
                if let Some(rv) = self.webn.rv_root.as_ref() {
                    self.status_box.push(LI::new("Loading finished"));
                    self.web
                        .get_element::<HtmlButtonElement>("loading-info-button")
                        .click();
                    self.web
                        .get_element::<HtmlElement>("home_page")
                        .set_hidden(false);
                    self.set_state(StateEnum::ShowPage(rv.clone())).await;
                }
            }
            StateEnum::ShowPage(_) => {
                self.set_state(StateEnum::Idle).await;
            }
            StateEnum::Idle => {}
        }
    }

    async fn set_state(&mut self, new_state: StateEnum) {
        match &new_state {
            StateEnum::Init => {}
            StateEnum::ConnectingSignalling => {
                self.status_box = UL::default();
                self.status_box
                    .push(LI::new("Connecting to signalling server"));
            }
            StateEnum::ConnectingNodes => {
                for div in ["node-stats", "global-stats"].iter() {
                    self.web
                        .get_element::<HtmlElement>(div)
                        .class_list()
                        .add_1("show")
                        .expect("add show");
                }
                self.status_box.push(LI::new("Connecting to other nodes"))
            }
            StateEnum::UpdateDHT => {
                self.web.unhide("menu-chat");
                self.web.unhide("menu-proxy");
                self.status_box.push(LI::new("Updating pages"));
                Chat::new(self.webn.gossip.clone(), self.tx.subscribe())
                    .await
                    .expect("Creating Chat");
                Proxy::new(
                    self.webn.node.webproxy.as_ref().unwrap().clone(),
                    self.webn.node.stat.as_ref().unwrap().clone(),
                    self.tx.subscribe(),
                )
                .expect("Creating Proxy");
            }
            StateEnum::ShowPage(rv) => {
                let verifier = FloVerifier::new(
                    rv.realm.realm_id(),
                    self.webn.node.crypto_storage.get_signer().verifier(),
                );
                self.webn.dht_storage.store_flo(verifier.into()).expect("Storing verifier");
                self.web.unhide("menu-page-edit");
                Pages::new(
                    rv.clone(),
                    self.tx.subscribe(),
                    self.webn.node.crypto_storage.get_signer(),
                )
                .await
                .expect("Initializing Pages");
            }
            StateEnum::Idle => {}
        }
        self.state = new_state;
        self.web
            .get_element::<HtmlOListElement>("status-steps")
            .set_inner_html(&self.status_box.to_string());
    }

    async fn clicked(&mut self, btn: Button) {
        match btn {
            Button::_DownloadData => {
                let data = self
                    .webn
                    .gossip
                    .storage
                    .borrow()
                    .get()
                    .expect("Dumping events");
                downloadFile("gossip_event.yaml".into(), data.into());
            }
            _ => {}
        }
    }
}

/// WebNode has all access for the node.
#[derive(Debug)]
pub struct WebNode {
    node: Node,
    _dht_storage_stats: watch::Receiver<dht_storage::messages::Stats>,
    realm_views: HashMap<RealmID, Option<RealmView>>,
    rv_root: Option<RealmView>,
    dht_storage: DHTStorage,
    dht_router: DHTRouter,
    gossip: Gossip,
    stats: NetStats,
    // page_fetcher: PageFetcher,
    counter: u32,
}

impl WebNode {
    pub async fn new() -> Result<Self> {
        log::info!("Starting new WebNode on {NETWORK_CONFIG:?}");
        let node = Self::node_start().await?;

        Ok(Self {
            _dht_storage_stats: node.dht_storage.as_ref().unwrap().stats.clone(),
            dht_storage: node.dht_storage.as_ref().unwrap().clone(),
            dht_router: node.dht_router.as_ref().unwrap().clone(),
            realm_views: HashMap::new(),
            rv_root: None,
            gossip: node.gossip.as_ref().unwrap().clone(),
            node,
            stats: NetStats::default(),
            counter: 0,
        })
    }

    pub async fn get_values(&mut self) -> Values {
        self.counter += 1;
        if self.counter < 10 || self.counter % 30 == 0 {
            if let Err(e) = self.dht_storage.sync() {
                log::warn!("While synching to other nodes: {e:?}");
            }
            if let Err(e) = self.node.request_list().await {
                log::warn!("Couldn't send request for a new list: {e:?}");
            }
        }
        if let Ok(realms) = self.dht_storage.get_realm_ids().await {
            self.update_realms(realms).await;
        }
        self.update_pages().await;
        self.stats = self.node.stat.as_ref().unwrap().borrow().clone();
        Values::new(&self.node)
    }

    async fn update_realms(&mut self, realms: Vec<RealmID>) {
        for realm in realms {
            if !self.realm_views.contains_key(&realm) {
                if let Ok(rv) =
                    RealmView::new_from_id(self.dht_storage.clone(), realm.clone()).await
                {
                    if self.rv_root.is_none()
                        && rv.realm.cache().get_services().contains_key("http")
                    {
                        self.rv_root = Some(rv.clone());
                    }
                    self.realm_views.insert(realm, Some(rv));
                } else {
                    self.realm_views.insert(realm, None);
                }
            }
        }
    }

    async fn update_pages(&mut self) {
        for (_, rvo) in &mut self.realm_views {
            if let Some(rv) = rvo {
                if let Err(e) = rv.update_pages().await {
                    log::warn!("While updating pages: {e:?}");
                }
            }
        }
    }

    async fn node_start() -> Result<Node> {
        let my_storage = DataStorageLocal::new("fledger");
        let mut node_config = Node::get_config(my_storage.clone())?;
        let config = ConnectionConfig::new(
            NETWORK_CONFIG.signal_server.into(),
            NETWORK_CONFIG
                .stun_server
                .and_then(|url| Some(HostLogin::from_url(url))),
            NETWORK_CONFIG
                .turn_server
                .and_then(|url| HostLogin::from_login_url(url).ok()),
        );
        node_config.info.modules = Modules::stable() - Modules::WEBPROXY_REQUESTS;
        Ok(Node::start_network(my_storage, node_config, config)
            .await
            .map_err(|e| anyhow!("Couldn't create node: {:?}", e))?)
    }
}
