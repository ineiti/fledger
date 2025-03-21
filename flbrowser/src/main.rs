use anyhow::{anyhow, Result};
use chrono::{prelude::DateTime, Utc};
use flcrypto::signer::KeyPairID;
use flmodules::{
    dht_router,
    dht_storage::{self, broker::DHTStorage, realm_view::RealmView},
    flo::{
        blob::{BlobID, FloBlobPage},
        realm::RealmID,
    },
    nodeconfig::NodeInfo,
    ping::core::{PingStat, PingStorage},
    Modules,
};
use itertools::Itertools;
use js_sys::JsString;
use regex::Regex;
use std::{
    collections::HashMap,
    mem::ManuallyDrop,
    time::{Duration, UNIX_EPOCH},
};
use tokio::sync::{
    mpsc::{UnboundedReceiver, UnboundedSender},
    watch,
};
use wasm_bindgen::{
    prelude::{wasm_bindgen, Closure},
    JsCast,
};
use web_sys::{
    window, Document, Event, HtmlButtonElement, HtmlDivElement, HtmlElement, HtmlInputElement,
    HtmlOListElement, HtmlTextAreaElement,
};

use flarch::{
    data_storage::DataStorageLocal,
    nodeids::{NodeID, U256},
    tasks::{spawn_local_nosend, wait_ms},
    web_rtc::connection::{ConnectionConfig, HostLogin},
};
use flmodules::network::broker::NetworkConnectionState;
use flmodules::network::network_start;
use flnode::{node::Node, version::VERSION_STRING};

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

#[derive(Debug, Clone)]
enum Button {
    SendMsg,
    _DownloadData,
    WebProxy,
    SavePage,
}

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
    node: WebNode,
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
            node: WebNode::new().await?,
            web,
            state: StateEnum::Init,
            status_box: UL(vec![]),
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
            let values = ws.node.get_values().await;
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
                if let Ok(nbr) = self.node.node.nodes_online() {
                    if nbr.len() > 0 {
                        self.set_state(StateEnum::ConnectingNodes).await;
                    }
                }
            }
            StateEnum::ConnectingNodes => {
                if let Ok(nbr) = self.node.node.nodes_online() {
                    if nbr.len() >= 2 {
                        self.set_state(StateEnum::UpdateDHT).await;
                    }
                }
            }
            StateEnum::UpdateDHT => {
                if let Some(dp) = self.node.get_dht_page_first() {
                    self.status_box.0.push(LI("Loading finished".into(), None));
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
                self.status_box = UL(vec![LI("Connecting to signalling server".into(), None)]);
                self.web
                    .get_input("page-path")
                    .set_value(&names::Generator::default().next().unwrap());
            }
            StateEnum::ConnectingNodes => self
                .status_box
                .0
                .push(LI("Connecting to other nodes".into(), None)),
            StateEnum::UpdateDHT => self.status_box.0.push(LI("Updating pages".into(), None)),
            StateEnum::ShowPage(dp) => {
                self.web.set_id_inner("dht_page", &dp.page.get_index());
                self.web.set_id_inner(
                    "dht_page_path",
                    &format!("{}/{}", dp.realm, dp.path.clone()),
                );
                let mut our_pages = vec![];
                for (_, fp) in self
                    .node
                    .realm_views
                    .get(&dp.realm)
                    .and_then(|pt| pt.pages.as_ref().map(|p| &p.storage))
                    .unwrap()
                {
                    if self
                        .node
                        .node
                        .dht_storage
                        .as_mut()
                        .unwrap()
                        .convert(fp.cond(), &fp.realm_id())
                        .await
                        .can_verify(&KeyPairID::rnd())
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
                    self.node
                        .node
                        .add_chat_message(msg)
                        .await
                        .expect("Should add chat message");
                }
            }
            Button::_DownloadData => {
                let data = self.node.get_gossip_data();
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
                    let mut webproxy = self.node.node.webproxy.as_ref().unwrap().clone();
                    let nodes = self.node.node.nodes_connected();
                    spawn_local_nosend(async move {
                        let fetching = format!("Fetching url from proxy: {}", url);
                        proxy_div.set_inner_html(&fetching);
                        match webproxy.get(&url).await {
                            Ok(mut response) => {
                                let mut proxy_str = format!("{}", response.proxy());
                                if let Ok(nodes) = nodes {
                                    if let Some(info) =
                                        nodes.iter().find(|&node| node.get_id() == response.proxy())
                                    {
                                        proxy_str = format!("{} ({})", info.name, info.get_id());
                                    }
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
        self.values.get_or_insert(self.node.get_values().await)
    }
}

/// WebNode has all access for the node.
#[derive(Debug)]
pub struct WebNode {
    node: Node,
    dht_storage_stats: watch::Receiver<dht_storage::messages::Stats>,
    realm_views: HashMap<RealmID, RealmView>,
    dht_storage: DHTStorage,
    // page_fetcher: PageFetcher,
    counter: u32,
}

impl WebNode {
    pub async fn new() -> Result<Self> {
        log::info!("Starting new WebNode on {NETWORK_CONFIG:?}");
        let node = Self::node_start().await?;

        Ok(Self {
            dht_storage_stats: node.dht_storage.as_ref().unwrap().stats.clone(),
            dht_storage: node.dht_storage.as_ref().unwrap().clone(),
            realm_views: HashMap::new(),
            node,
            counter: 0,
        })
    }

    pub async fn get_values(&mut self) -> Values {
        self.counter += 1;
        if self.counter < 10 || self.counter % 30 == 0 {
            if let Err(e) = self.node.dht_storage.as_mut().unwrap().sync() {
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
        Values::new(&self.node)
    }

    async fn update_realms(&mut self, realms: Vec<RealmID>) {
        for realm in realms {
            if !self.realm_views.contains_key(&realm) {
                if let Ok(rv) =
                    RealmView::new_from_id(self.dht_storage.clone(), realm.clone()).await
                {
                    self.realm_views.insert(realm, rv);
                }
            }
        }
    }

    async fn update_pages(&mut self) {
        for (_, rv) in &mut self.realm_views {
            if let Err(e) = rv.update_pages().await {
                log::warn!("While updating pages: {e:?}");
            }
        }
    }

    pub fn get_dht_page_first(&self) -> Option<DhtPage> {
        self.dht_storage_stats
            .borrow()
            .system_realms
            .iter()
            .chain(self.realm_views.keys())
            .filter_map(|rid| self.get_dht_page_root(rid))
            .next()
    }

    pub fn get_dht_page_root(&self, rid: &RealmID) -> Option<DhtPage> {
        self.realm_views.get(rid).and_then(|rv| {
            rv.realm
                .cache()
                .get_services()
                .get("http")
                .and_then(|root_id| self.get_dht_page(rid, &(**root_id).into()))
        })
    }

    pub fn get_dht_page(&self, rid: &RealmID, page_id: &BlobID) -> Option<DhtPage> {
        self.realm_views.get(rid).and_then(|rv| {
            rv.pages.as_ref().and_then(|pages| {
                pages
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
        self.node
            .gossip
            .as_ref()
            .unwrap()
            .storage
            .borrow()
            .get()
            .unwrap()
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

/// Web interfaces with the html code.
#[derive(Debug)]
pub struct Web {
    document: Document,
    tx: UnboundedSender<Button>,
    rx: UnboundedReceiver<Button>,
}

#[derive(Debug)]
pub struct DhtPage {
    realm: RealmID,
    page: FloBlobPage,
    path: String,
}

impl Web {
    pub async fn new() -> Result<Self> {
        Web::set_data_storage();
        log::info!("Starting Web");

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Button>();
        let window = web_sys::window().expect("no global `window` exists");
        Ok(Self {
            document: window.document().expect("should have a document on window"),
            tx,
            rx,
        })
    }

    fn link_btn(&self, btn: Button, id: &str) {
        let tx = self.tx.clone();
        let cb = ManuallyDrop::new(Closure::wrap(Box::new(move |_: Event| {
            tx.send(btn.clone())
                .err()
                .map(|e| log::error!("Couldn't send message: {e:?}"));
        }) as Box<dyn FnMut(_)>));
        self.document.get_element_by_id(id).map_or_else(
            || {
                log::warn!("Couldn't find button with id: {id}");
            },
            |el| {
                el.add_event_listener_with_callback("click", &cb.as_ref().unchecked_ref())
                    .expect("Should be able to add event listener");
            },
        );
    }

    fn set_id_inner(&self, id: &str, inner_html: &str) {
        if self
            .document
            .get_element_by_id(id)
            .map(|el| el.set_inner_html(inner_html))
            .is_none()
        {
            log::warn!("Couldn't set inner html for id: {}", id);
        }
    }

    fn _scroll_to_bottom(&self, id: &str) {
        if let Some(messages_element) = self.document.get_element_by_id(id) {
            messages_element
                .dyn_ref::<web_sys::HtmlElement>()
                .map(|el| el.set_scroll_top(el.scroll_height()));
        }
    }

    fn get_element<ET: JsCast>(&self, id: &str) -> ET {
        self.document
            .get_element_by_id(id)
            .unwrap()
            .dyn_into::<ET>()
            .map_err(|e| log::error!("Couldn't get element {id}: {e:?}"))
            .unwrap()
    }

    fn get_text_area(&self, id: &str) -> HtmlTextAreaElement {
        self.get_element(id)
    }

    fn get_div(&self, id: &str) -> HtmlDivElement {
        self.get_element(id)
    }

    fn get_input(&self, id: &str) -> HtmlInputElement {
        self.get_element(id)
    }

    fn get_button(&self, id: &str) -> HtmlButtonElement {
        self.get_element(id)
    }

    pub fn udpate_values(&mut self, val: &Values) {
        self.set_id_inner("node_info", &val.get_node_name());
        self.set_id_inner("username_display", &val.get_node_name());
        self.set_id_inner("version", &val.get_version());
        self.set_id_inner("nodes_online", &format!("{}", val.nodes_online));
        self.set_id_inner("nodes_online_random", &format!("{}", val.nodes_online));
        self.set_id_inner("nodes_connected", &format!("{}", val.nodes_connected));
        self.set_id_inner("msgs_system", &format!("{}", val.msgs_system));
        self.set_id_inner("msgs_local", &format!("{}", val.msgs_local));
        // self.set_html_id("dht_stats", &val.get_dht_stats());
        self.set_id_inner("dht_connections", &val.dht_router.active.to_string());
        self.set_id_inner(
            "realms_count",
            &val.dht_storage.realm_stats.len().to_string(),
        );
        self.set_id_inner(
            "dht_storage_local",
            &human_readable_size(val.dht_storage_local()),
        );
        self.set_id_inner(
            "dht_storage_limit",
            &human_readable_size(val.dht_storage_max()),
        );
        self.set_id_inner("connected_stats", &val.connected_stats());
    }

    fn set_editable_pages(&mut self, _pages: &Vec<FloBlobPage>) {
        // r#"
        //     <li>
        //         <span>/example/path</span>
        //         <button class="edit-btn" title="Edit Page"><i class="fas fa-edit"></i></button>
        //         <button class="view-btn" title="View Page"><i class="fas fa-eye"></i></button>
        //     </li>
        // "#;
    }

    fn set_data_storage() {
        if let Ok(loc) = window().unwrap().location().href() {
            if loc.contains('#') {
                let reg = Regex::new(r".*?#").unwrap();
                let data_enc = reg.replace(&loc, "");
                if data_enc != "" {
                    if let Ok(data) = urlencoding::decode(&data_enc) {
                        if let Err(err) = Node::set_config(DataStorageLocal::new("fledger"), &data)
                        {
                            log::warn!("Got error while saving config: {}", err);
                        }
                    }
                }
            }
        }
    }

    fn get_chat_msg(&self) -> String {
        let your_message = self.get_text_area("your_message");
        let msg = your_message.value();
        your_message.set_value("");
        msg
    }
}

#[derive(Debug)]
pub struct Values {
    info: NodeInfo,
    nodes_info: HashMap<U256, NodeInfo>,
    states: HashMap<U256, NetworkConnectionState>,
    pings: PingStorage,
    msgs: FledgerMessages,
    dht_storage: dht_storage::messages::Stats,
    dht_router: dht_router::messages::Stats,
    pub msgs_system: usize,
    pub msgs_local: usize,
    pub mana: u32,
    pub nodes_online: usize,
    pub nodes_connected: usize,
}

#[derive(Clone, Debug)]
struct UL(Vec<LI>);

#[derive(Clone, Debug)]
struct LI(String, Option<UL>);

impl UL {
    fn to_string(&self) -> String {
        format!(
            "<ul>{}</ul>",
            self.0
                .clone()
                .into_iter()
                .map(|hl| hl.to_string())
                .collect::<Vec<_>>()
                .join("")
        )
    }
}

impl LI {
    fn to_string(self) -> String {
        format!(
            "<li>{}{}</li>",
            self.0,
            self.1.map(|ul| ul.to_string()).unwrap_or("".to_string())
        )
    }
}

impl Values {
    fn new(node: &Node) -> Self {
        let info = node.node_config.info.clone();
        let msgs = node.gossip.as_ref().unwrap().chat_events();
        let nodes_info = node.nodes_info_all().unwrap();
        Self {
            nodes_online: node.nodes_online().unwrap().len(),
            nodes_connected: node.nodes_connected().unwrap().len(),
            msgs_system: msgs.len(),
            msgs_local: msgs.len(),
            mana: 0,
            msgs: FledgerMessages::new(
                info.get_id(),
                msgs,
                &nodes_info.clone().into_values().collect(),
            ),
            nodes_info,
            dht_router: node.dht_router.as_ref().unwrap().stats.borrow().clone(),
            dht_storage: node.dht_storage.as_ref().unwrap().stats.borrow().clone(),
            states: node.stat.as_ref().unwrap().borrow().clone(),
            pings: node.ping.as_ref().unwrap().storage.borrow().clone(),
            info,
        }
    }

    pub fn get_node_name(&self) -> String {
        self.info.name.clone()
    }

    pub fn get_node_table(&self) -> String {
        match self.get_node_table_result() {
            Ok(res) => res,
            Err(_) => String::from(""),
        }
    }

    pub fn get_version(&self) -> String {
        VERSION_STRING.to_string()
    }

    pub fn get_msgs(&self) -> String {
        self.msgs.get_messages()
    }

    pub fn get_dht_stats(&self) -> String {
        let mut out = UL(vec![LI(
            format!(
                "Other nodes available: {}",
                self.dht_router
                    .bucket_nodes
                    .iter()
                    .map(|b| format!("{}", b.len()))
                    .collect::<Vec<_>>()
                    .join(" - ")
            ),
            None,
        )]);
        for (rid, stats) in &self.dht_storage.realm_stats {
            let buckets = stats
                .distribution
                .iter()
                .map(|s| format!("{s}"))
                .collect::<Vec<_>>()
                .join(" - ");
            out.0.push(LI(
                format!("Realm {rid}:"),
                Some(UL(vec![
                    LI(
                        format!("Memory usage: {} of {}", stats.size, stats.config.max_space),
                        None,
                    ),
                    LI(format!("Number of Flos (blobs): {}", stats.flos), None),
                    LI(format!("Bucket distribution of Flos: {}", buckets), None),
                ])),
            ))
        }
        out.to_string()
    }

    fn get_nodes(&self) -> Vec<NodeDesc> {
        let mut out = vec![];
        let mut nodes: Vec<(&U256, &NodeInfo)> =
            self.nodes_info.iter().map(|(k, v)| (k, v)).collect();
        nodes.sort_by(|a, b| a.1.name.partial_cmp(&b.1.name).unwrap());
        for (id, ni) in &nodes {
            let stat = self
                .states
                .get(id)
                .map(|s| format!("{:?}", s.s.type_local))
                .unwrap_or("n/a".into());
            let info = ni.name.clone();
            if let Some(ping) = self.pings.stats.get(id) {
                out.push(NodeDesc {
                    info,
                    ping: ping.clone(),
                    stat,
                })
            }
        }
        out
    }

    fn get_node_table_result(&self) -> Result<String> {
        let stats_vec: Vec<String> = self
            .get_nodes()
            .into_iter()
            .map(|node| {
                vec![
                    node.info,
                    format!("rx:{} tx:{}", node.ping.rx, node.ping.tx),
                    node.ping.lastping.to_string(),
                    node.stat,
                ]
                .join("</td><td>")
            })
            .collect();
        Ok(format!(
            "<tr><td>{}</td></tr>",
            stats_vec.join("</td></tr><tr><td>")
        ))
    }

    fn dht_storage_local(&self) -> usize {
        self.dht_storage
            .realm_stats
            .iter()
            .map(|s| s.1.size)
            .sum::<usize>()
    }

    fn dht_storage_max(&self) -> usize {
        self.dht_storage
            .realm_stats
            .iter()
            .map(|s| s.1.config.max_space)
            .sum::<u64>() as usize
    }

    fn connected_stats(&self) -> String {
        self.states
            .iter()
            .map(|s| {
                format!(
                    "{} - {:?}",
                    self.nodes_info
                        .get(s.0)
                        .map(|ni| format!("{}", ni.name))
                        .unwrap_or(format!("{}", s.0)),
                    s.1.s.type_local
                )
            })
            .sorted()
            .collect::<Vec<String>>()
            .join("<br>")
    }
}

struct NodeDesc {
    info: String,
    ping: PingStat,
    stat: String,
}

#[derive(Clone, Debug)]
pub struct FledgerMessage {
    from: String,
    date: String,
    text: String,
    our_message: bool,
}

#[derive(Clone, Debug)]
pub struct FledgerMessages {
    msgs: Vec<FledgerMessage>,
}

impl FledgerMessages {
    fn new(
        our_id: NodeID,
        mut tm_msgs: Vec<flmodules::gossip_events::core::Event>,
        nodes: &Vec<NodeInfo>,
    ) -> Self {
        tm_msgs.sort_by(|a, b| a.created.partial_cmp(&b.created).unwrap());
        let mut msgs = vec![];
        for msg in tm_msgs {
            let d = UNIX_EPOCH + Duration::from_secs(msg.created as u64 / 1000);
            // Create DateTime from SystemTime
            let datetime = DateTime::<Utc>::from(d);
            // Formats the combined date and time with the specified format string.
            let date = datetime.format("%A, the %d of %B at %H:%M:%S").to_string();

            let node: Vec<&NodeInfo> = nodes.iter().filter(|&ni| ni.get_id() == msg.src).collect();
            let from = if node.len() == 1 {
                node[0].name.clone()
            } else {
                format!("{}", msg.src)
            };
            msgs.push(FledgerMessage {
                our_message: our_id == msg.src,
                from,
                text: msg.msg.clone(),
                date,
            })
        }
        FledgerMessages { msgs }
    }

    pub fn get_messages(&self) -> String {
        if self.msgs.is_empty() {
            return String::from("No messages");
        }
        self.msgs
            .iter()
            .map(|fm| fm.to_string())
            .collect::<Vec<String>>()
            .join("")
    }
}

impl FledgerMessage {
    pub fn to_string(&self) -> String {
        format!(
            r#"{}
                <div class="message-sender">{}</div>
                <div class="message-content">{}</div>
                <div class="message-time">{}</div>
            </div>
            "#,
            if self.our_message {
                format!(r#"<div class="message-item sent">"#)
            } else {
                format!(r#"<div class="message-item received">"#)
            },
            self.from,
            self.text,
            self.date
        )
    }
}

fn human_readable_size(size: usize) -> String {
    if size == 0 {
        return "0 B".to_string();
    }
    let units = ["B", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];
    let size = size as f64;
    let i = (size.ln() / 1024_f64.ln()).floor() as i32;
    let size = size / 1024_f64.powi(i);
    format!("{:.2} {}", size, units[i as usize])
}
