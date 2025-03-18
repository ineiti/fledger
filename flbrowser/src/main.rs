use anyhow::{anyhow, Result};
use chrono::{prelude::DateTime, Utc};
use flmodules::{
    dht_router, dht_storage,
    flo::realm::RealmID,
    nodeconfig::NodeInfo,
    ping::core::{PingStat, PingStorage},
    Modules,
};
use itertools::Itertools;
use js_sys::JsString;
use realm_pages::{PageFetcher, PageTags};
use regex::Regex;
use std::{
    collections::HashMap,
    mem::ManuallyDrop,
    time::{Duration, UNIX_EPOCH},
};
use tokio::sync::mpsc::UnboundedSender;
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

mod realm_pages;

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
    DownloadData,
    WebProxy,
}

#[wasm_bindgen(module = "/src/main.js")]
extern "C" {
    fn downloadFile(fileName: JsString, data: JsString);
}

// Because I really want to have a 'normal' HTML file and then link it with rust,
// this code is necessary to link the two.
// Using things like Yew or others is too far from HTML for me.
// Any suggestions for a framework that allows to do this in a cleaner way are welcome.
pub fn main() {
    spawn_local_nosend(async {
        let mut web = FledgerWeb::new().await.expect("Should run fledger");

        // Create a listening channel that fires whenever the user clicks on the `send_msg` button
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Button>();
        web.link_btn(tx.clone(), Button::SendMsg, "send_message");
        web.link_btn(tx.clone(), Button::DownloadData, "get_data");
        web.link_btn(tx, Button::WebProxy, "proxy_request");

        // Link to some elements
        let your_message: HtmlTextAreaElement = web.get_element("your_message");
        let proxy_div: HtmlDivElement = web.get_element("proxy_div");
        let status_steps: HtmlOListElement = web.get_element("status-steps");
        let loading_info_button: HtmlButtonElement = web.get_element("loading-info-button");
        let home_page: HtmlElement = web.get_element("home_page");
        home_page.set_hidden(true);
        let proxy_url: HtmlInputElement = web.get_element("proxy_url");
        let webproxy = web.node.webproxy.as_mut().unwrap().clone();

        let mut status = UL(vec![LI("Connecting to signalling server".into(), None)]);

        loop {
            if let Ok(btn) = rx.try_recv() {
                match btn {
                    Button::SendMsg => {
                        // User clicked on the `send_msg` button
                        let msg = your_message.value();
                        if msg != "" {
                            web.node
                                .add_chat_message(msg)
                                .await
                                .expect("Should add chat message");
                            your_message.set_value("");
                        }
                    }
                    Button::DownloadData => {
                        let data = web
                            .node
                            .gossip
                            .as_ref()
                            .unwrap()
                            .storage
                            .borrow()
                            .get()
                            .unwrap();
                        downloadFile("gossip_event.yaml".into(), data.into());
                    }
                    Button::WebProxy => {
                        let proxy_div = proxy_div.clone();
                        let proxy_url = proxy_url.value();
                        let mut webproxy = webproxy.clone();
                        let nodes = web.node.nodes_connected();
                        spawn_local_nosend(async move {
                            let fetching = format!("Fetching url from proxy: {}", proxy_url);
                            proxy_div.set_inner_html(&fetching);
                            match webproxy.get(&proxy_url).await {
                                Ok(mut response) => {
                                    let mut proxy_str = format!("{}", response.proxy());
                                    if let Ok(nodes) = nodes {
                                        if let Some(info) = nodes
                                            .iter()
                                            .find(|&node| node.get_id() == response.proxy())
                                        {
                                            proxy_str =
                                                format!("{} ({})", info.name, info.get_id());
                                        }
                                    }
                                    let text = format!(
                                        "Proxy: {proxy_str}<br>{}",
                                        response.text().await.unwrap()
                                    );
                                    proxy_div.set_inner_html(&text);
                                }
                                Err(e) => {
                                    let text =
                                        format!("Got error while fetching page from proxy: {e}");
                                    proxy_div.set_inner_html(&text);
                                }
                            }
                        });
                    }
                }
            }
            status_steps.set_inner_html(&status.to_string());

            if let Some(state) = web.tick().await {
                if state.page_tags.len() > 0 && status.0.len() == 3 {
                    // loading_info.set_hidden(true);
                    loading_info_button.click();
                    home_page.set_hidden(false);
                    status.0.push(LI("Loading finished".into(), None));
                }
                if state.nodes_connected >= 2 && status.0.len() == 2 {
                    status.0.push(LI("Updating pages".into(), None));
                }
                if status.0.len() == 1 {
                    status.0.push(LI("Connecting to other nodes".into(), None));
                }
                web.set_html_id("node_info", state.get_node_name());
                web.set_html_id("username_display", state.get_node_name());
                web.set_html_id("version", state.get_version());
                web.set_html_id("messages", state.get_msgs());
                web.set_html_id("nodes_online", format!("{}", state.nodes_online));
                web.set_html_id("nodes_online_random", format!("{}", state.nodes_online));
                web.set_html_id("nodes_connected", format!("{}", state.nodes_connected));
                web.set_html_id("msgs_system", format!("{}", state.msgs_system));
                web.set_html_id("msgs_local", format!("{}", state.msgs_local));
                // web.set_html_id("dht_stats", state.get_dht_stats());
                web.set_html_id("dht_page", state.get_dht_pages());
                web.set_html_id("dht_connections", state.dht_router.active.to_string());
                web.set_html_id(
                    "realms_count",
                    state.dht_storage.realm_stats.len().to_string(),
                );
                web.set_html_id(
                    "dht_storage_local",
                    human_readable_size(
                        state
                            .dht_storage
                            .realm_stats
                            .iter()
                            .map(|s| s.1.size)
                            .sum::<usize>(),
                    ),
                );
                web.set_html_id(
                    "dht_storage_limit",
                    human_readable_size(
                        state
                            .dht_storage
                            .realm_stats
                            .iter()
                            .map(|s| s.1.config.max_space)
                            .sum::<u64>() as usize,
                    ),
                );
                web.set_html_id(
                    "connected_stats",
                    state
                        .states
                        .iter()
                        .map(|s| {
                            format!(
                                "{} - {:?}",
                                state
                                    .nodes_info
                                    .get(s.0)
                                    .map(|ni| format!("{}", ni.name))
                                    .unwrap_or(format!("{}", s.0)),
                                s.1.s.type_local
                            )
                        })
                        .sorted()
                        .collect::<Vec<String>>()
                        .join("<br>"),
                );
                // dht-pages-own - our pages
                // dht-pages-total - all pages
                // dht-storage-local
                // dht-storage-max
                // dht-router-connections - bucket list of how many connections this node has
                // dht-storage-buckets - how many flos are in each bucket
                // total-data - estimation of how much data is stored in the system
                // nodes-browser / nodes/cli - browser- and cli-nodes
                // realms-number - number of realms
            }
            wait_ms(1000).await;
        }
    });
}

/// FledgerWeb is a nearly generic handler for the FledgerNode.
pub struct FledgerWeb {
    node: Node,
    document: Document,
    counter: u32,
    page_fetcher: PageFetcher,
}

impl FledgerWeb {
    pub async fn new() -> Result<Self> {
        console_error_panic_hook::set_once();
        FledgerWeb::set_data_storage();

        wasm_logger::init(wasm_logger::Config::new(log::Level::Debug));
        log::info!("Starting new FledgerWeb on {NETWORK_CONFIG:?}");

        // Get a link to the document
        let window = web_sys::window().expect("no global `window` exists");
        let node = Self::node_start().await?;
        Ok(Self {
            page_fetcher: PageFetcher::new(node.dht_storage.as_ref().unwrap().clone()).await,
            node,
            document: window.document().expect("should have a document on window"),
            counter: 0u32,
        })
    }

    fn link_btn(&self, tx: UnboundedSender<Button>, btn: Button, id: &str) {
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

    fn set_html_id(&self, id: &str, inner_html: String) {
        if self
            .document
            .get_element_by_id(id)
            .map(|el| el.set_inner_html(&inner_html))
            .is_none()
        {
            log::info!("Couldn't set inner html for id: {}", id);
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

    pub async fn tick(&mut self) -> Option<FledgerState> {
        let mut fs = None;
        if let Err(e) = self.node.dht_storage.as_mut().unwrap().sync() {
            log::warn!("While synching to other nodes: {e:?}");
        }
        match FledgerState::new(&self.node, self.page_fetcher.page_tags_rx.borrow().clone()) {
            Ok(f) => fs = Some(f),
            Err(e) => log::error!("Couldn't create state: {:?}", e),
        }
        self.counter += 1;
        self.node
            .request_list()
            .await
            .expect("Couldn't request list");
        fs
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
}

pub struct FledgerState {
    info: NodeInfo,
    nodes_info: HashMap<U256, NodeInfo>,
    states: HashMap<U256, NetworkConnectionState>,
    pings: PingStorage,
    msgs: FledgerMessages,
    dht_storage: dht_storage::messages::Stats,
    dht_router: dht_router::messages::Stats,
    page_tags: HashMap<RealmID, PageTags>,
    pub msgs_system: usize,
    pub msgs_local: usize,
    pub mana: u32,
    pub nodes_online: usize,
    pub nodes_connected: usize,
}

#[derive(Clone)]
struct UL(Vec<LI>);

#[derive(Clone)]
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

impl FledgerState {
    fn new(node: &Node, page_tags: HashMap<RealmID, PageTags>) -> Result<Self> {
        let info = node.node_config.info.clone();
        let msgs = node.gossip.as_ref().unwrap().chat_events();
        let nodes_info = node.nodes_info_all()?;
        Ok(Self {
            nodes_online: node.nodes_online()?.len(),
            nodes_connected: node.nodes_connected()?.len(),
            msgs_system: msgs.len(),
            msgs_local: msgs.len(),
            mana: 0,
            msgs: FledgerMessages::new(
                info.get_id(),
                msgs,
                &nodes_info.clone().into_values().collect(),
            ),
            nodes_info,
            page_tags,
            dht_router: node.dht_router.as_ref().unwrap().stats.borrow().clone(),
            dht_storage: node.dht_storage.as_ref().unwrap().stats.borrow().clone(),
            states: node.stat.as_ref().unwrap().borrow().clone(),
            pings: node.ping.as_ref().unwrap().storage.borrow().clone(),
            info,
        })
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

    pub fn get_dht_pages(&self) -> String {
        let mut out = UL(vec![]);
        for (rid, rv) in self.page_tags.iter().sorted_by_key(|(id, _)| *id) {
            let index = rv.pages.as_ref().map(|fp| {
                UL(vec![LI(
                    format!(
                        "id: {} - {}",
                        fp.root.clone(),
                        fp.storage
                            .get(&fp.root)
                            .map(|blob| blob.get_index())
                            .unwrap_or("empty page".into())
                    ),
                    None,
                )])
            });
            out.0.push(LI(
                format!("Realm: {} / {}", rv.realm.cache().get_name(), rid),
                index,
            ));
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
}

struct NodeDesc {
    info: String,
    ping: PingStat,
    stat: String,
}

#[derive(Clone)]
pub struct FledgerMessage {
    from: String,
    date: String,
    text: String,
    our_message: bool,
}

#[derive(Clone)]
pub struct FledgerMessages {
    msgs: Vec<FledgerMessage>,
}

impl FledgerMessages {
    fn new(
        our_id: NodeID,
        mut tm_msgs: Vec<flmodules::gossip_events::core::Event>,
        nodes: &Vec<NodeInfo>,
    ) -> Self {
        tm_msgs.sort_by(|a, b| b.created.partial_cmp(&a.created).unwrap());
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
    let units = ["B", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];
    let size = size as f64;
    let i = (size.ln() / 1024_f64.ln()).floor() as i32;
    let size = size / 1024_f64.powi(i);
    format!("{:.2} {}", size, units[i as usize])
}
