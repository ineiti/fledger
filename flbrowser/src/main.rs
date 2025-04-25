use anyhow::{anyhow, Result};
use flcrypto::signer::KeyPairID;
use flmodules::{
    dht_router::broker::DHTRouter,
    dht_storage::{self, broker::DHTStorage, realm_view::RealmView},
    flo::{
        blob::{BlobID, BlobPath},
        realm::RealmID,
    },
    gossip_events::broker::Gossip,
    web_proxy::broker::WebProxy,
    Modules,
};
use js_sys::JsString;
use regex::Regex;
use std::collections::HashMap;
use tokio::sync::watch;
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

#[wasm_bindgen(module = "/src/main.js")]
extern "C" {
    fn downloadFile(fileName: JsString, data: JsString);
    fn getEditorContent() -> JsString;
}

// Because I really want to have a 'normal' HTML file and then link it with rust,
// this code is necessary to link the two.
// Using things like Yew or others is too far from HTML for me.
// Any suggestions for a framework that allows to do this in a cleaner way are welcome.
pub fn main() {
    console_error_panic_hook::set_once();
    wasm_logger::init(wasm_logger::Config::new(log::Level::Debug));

    spawn_local_nosend(async {
        if let Err(e) = WebState::start().await {
            log::error!("Error while executing fledger: {e:?}");
        }
    });
}

#[derive(Debug)]
pub struct WebState {
    webn: WebNode,
    web: Web,
    state: StateEnum,
    status_box: UL,
    values: Option<Values>,
    previous_chat: Option<String>,
}

#[derive(Debug)]
enum StateEnum {
    Init,
    ConnectingSignalling,
    ConnectingNodes,
    UpdateDHT,
    ShowPage(DhtPage),
    Idle,
}

impl WebState {
    pub async fn new() -> Result<Self> {
        // Need to make sure that Web is started first, as it allows to set the storage
        // from an html anchor.
        let web = Web::new().await?;
        Ok(Self {
            webn: WebNode::new().await?,
            web,
            state: StateEnum::Init,
            status_box: UL::default(),
            values: None,
            previous_chat: None,
        })
    }

    pub async fn start() -> Result<()> {
        let mut ws = Self::new().await?;
        ws.web.link_btn(Button::SendMsg, "send_message");
        // ws.web.link_btn(Button::DownloadData, "get_data");
        ws.web.link_btn(Button::WebProxy, "proxy_request");
        ws.web.link_btn(Button::SavePage, "save-page");

        loop {
            let values = ws.webn.get_values().await;
            ws.web.udpate_values(&values);
            ws.values = Some(values);
            ws.check_state().await;
            tokio::select! {
                _ = wait_ms(1000) => {},
                Some(btn) = ws.web.rx.recv() => {ws.clicked(btn).await;}
            }
        }
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
                if let Some(dp) = self.webn.get_dht_page_first() {
                    self.status_box.push(LI::new("Loading finished"));
                    self.web
                        .get_element::<HtmlButtonElement>("loading-info-button")
                        .click();
                    self.web
                        .get_element::<HtmlElement>("home_page")
                        .set_hidden(false);
                    self.set_state(StateEnum::ShowPage(dp)).await;
                }
            }
            StateEnum::ShowPage(_) => {
                self.set_state(StateEnum::Idle).await;
            }
            StateEnum::Idle => {
                let new_msgs = self.values().await.get_msgs();
                if Some(&new_msgs) != self.previous_chat.as_ref() {
                    self.web.set_id_inner("messages", &new_msgs);
                    self.previous_chat = Some(new_msgs);
                }
            }
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
                self.web.remove_hidden("menu-chat");
                self.web.remove_hidden("menu-proxy");
                self.status_box.push(LI::new("Updating pages"));
            }
            StateEnum::ShowPage(dp) => {
                self.web.remove_hidden("menu-page-edit");

                let path = format!(
                    "/{}/{}",
                    dp.page.get_path().unwrap_or(&"".to_string()),
                    names::Generator::default().next().unwrap()
                );
                self.web.get_input("page-path").set_value(&path);

                self.web.set_id_inner("dht_page", &dp.page.get_index());
                self.web.set_id_inner(
                    "dht_page_path",
                    &format!("{}/{}", dp.realm, dp.path.clone()),
                );
                let mut our_pages = vec![];
                for (_, fp) in self
                    .webn
                    .realm_views
                    .get(&dp.realm)
                    .and_then(|pt| pt.as_ref().map(|pt| &pt.pages.storage))
                    .unwrap()
                {
                    if self
                        .webn
                        .node
                        .dht_storage
                        .as_mut()
                        .unwrap()
                        .convert(fp.cond(), &fp.realm_id())
                        .await
                        .can_verify(&[&KeyPairID::rnd()])
                    {
                        our_pages.push(fp.clone());
                    }
                }
                self.web.set_editable_pages(&our_pages);
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
            Button::SendMsg => {
                // User clicked on the `send_msg` button
                let msg = self.web.get_chat_msg();
                if msg != "" {
                    self.webn
                        .gossip
                        .add_chat_message(msg)
                        .await
                        .expect("Should add chat message");
                }
            }
            Button::_DownloadData => {
                let data = self.webn.get_gossip_data();
                downloadFile("gossip_event.yaml".into(), data.into());
            }
            Button::WebProxy => {
                let proxy_url = self.web.get_input("proxy_url");
                if proxy_url.value() != "" {
                    let proxy_button = self.web.get_button("proxy_request");
                    proxy_button.set_disabled(true);
                    let mut url = proxy_url.value();
                    if !Regex::new("^https?://").unwrap().is_match(&url) {
                        url = format!("https://{url}");
                    }
                    let proxy_div = self.web.get_div("proxy_div");
                    let mut webproxy = self.webn.web_proxy.clone();
                    let nodes = self.webn.stats.node_infos_connected();
                    spawn_local_nosend(async move {
                        let fetching = format!("Fetching url from proxy: {}", url);
                        proxy_div.set_inner_html(&fetching);
                        match webproxy.get(&url).await {
                            Ok(mut response) => {
                                let mut proxy_str = format!("{}", response.proxy());
                                if let Some(info) =
                                    nodes.iter().find(|&node| node.get_id() == response.proxy())
                                {
                                    proxy_str = format!("{} ({})", info.name, info.get_id());
                                }
                                let text = format!(
                                    "Proxy: {proxy_str}<br>{}",
                                    response.text().await.unwrap()
                                );
                                proxy_div.set_inner_html(&text);
                            }
                            Err(e) => {
                                let text = format!("Got error while fetching page from proxy: {e}");
                                proxy_div.set_inner_html(&text);
                            }
                        }
                        proxy_button.set_disabled(false);
                    });
                }
            }
            Button::SavePage => {
                let page_path = self.web.get_input("page-path").value();
                let page_content: String = getEditorContent().into();

                log::info!("Page: {page_path} - {page_content}");
            }
        }
    }

    async fn values(&mut self) -> &Values {
        self.values.get_or_insert(self.webn.get_values().await)
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
    web_proxy: WebProxy,
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
            web_proxy: node.webproxy.as_ref().unwrap().clone(),
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
                    if self.rv_root.is_none() {
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

    pub fn get_dht_page_first(&self) -> Option<DhtPage> {
        self.rv_root
            .as_ref()
            .and_then(|rv| self.get_dht_page_root(&rv.realm.realm_id()))
    }

    pub fn get_dht_page_root(&self, rid: &RealmID) -> Option<DhtPage> {
        self.realm_views.get(rid).and_then(|rvo| {
            rvo.as_ref().and_then(|rv| {
                rv.realm
                    .cache()
                    .get_services()
                    .get("http")
                    .and_then(|root_id| self.get_dht_page(rid, &(**root_id).into()))
            })
        })
    }

    pub fn get_dht_page(&self, rid: &RealmID, page_id: &BlobID) -> Option<DhtPage> {
        self.realm_views.get(rid).and_then(|rvo| {
            rvo.as_ref().and_then(|rv| {
                rv.pages
                    .storage
                    .get(&(**page_id).into())
                    .map(|root_page| DhtPage {
                        realm: rid.clone(),
                        page: root_page.clone(),
                        path: rv.realm.cache().get_name(),
                    })
            })
        })
    }

    pub fn get_gossip_data(&self) -> String {
        self.gossip.storage.borrow().get().unwrap()
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
        let network = network_start(node_config.clone(), config).await?;
        node_config.info.modules = Modules::all() - Modules::WEBPROXY_REQUESTS;
        Ok(Node::start(my_storage, node_config, network.broker)
            .await
            .map_err(|e| anyhow!("Couldn't create node: {:?}", e))?)
    }
}
