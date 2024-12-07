use anyhow::{anyhow, Result};
use chrono::{prelude::DateTime, Utc};
use flmodules::{
    nodeconfig::NodeInfo,
    ping::core::{PingStat, PingStorage},
    Modules,
};
use js_sys::JsString;
use regex::Regex;
use std::{
    collections::HashMap,
    mem::ManuallyDrop,
    time::{Duration, UNIX_EPOCH},
};
use tokio::sync::mpsc::UnboundedSender;
use wasm_bindgen::{
    prelude::{wasm_bindgen, Closure},
    JsCast, JsValue,
};
use web_sys::{window, Document, Event, HtmlDivElement, HtmlInputElement, HtmlTextAreaElement};

use flarch::{
    data_storage::DataStorageLocal,
    nodeids::U256,
    tasks::{spawn_local_nosend, wait_ms},
    web_rtc::connection::{ConnectionConfig, HostLogin, Login},
};
use flmodules::network::messages::NetworkConnectionState;
use flmodules::network::network_broker_start;
use flnode::{node::Node, version::VERSION_STRING};

#[cfg(not(feature = "local"))]
const URL: &str = "wss://signal.fledg.re";

#[cfg(feature = "local")]
const URL: &str = "ws://localhost:8765";

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
        web.link_btn(tx.clone(), Button::SendMsg, "send_msg");
        web.link_btn(tx.clone(), Button::DownloadData, "get_data");
        web.link_btn(tx, Button::WebProxy, "proxy_request");

        // Link to some elements
        let your_message: HtmlTextAreaElement = web.get_element("your_message");
        let proxy_div: HtmlDivElement = web.get_element("proxy_div");
        let proxy_url: HtmlInputElement = web.get_element("proxy_url");
        let webproxy = web.node.webproxy.as_mut().unwrap().clone();

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
                        let data = web.node.gossip.as_ref().unwrap().storage.get().unwrap();
                        downloadFile("gossip_event.toml".into(), data.into());
                    }
                    Button::WebProxy => {
                        let proxy_div = proxy_div.clone();
                        let proxy_url = proxy_url.value();
                        let mut webproxy = webproxy.clone();
                        let nodes = web.node.nodes_connected();
                        spawn_local_nosend(async move {
                            let fetching =
                                format!("Fetching url from proxy: {}", proxy_url);
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
            if let Some(state) = web.tick().await {
                update_table(&web, &state)
                    .err()
                    .map(|_| log::error!("While updating table"));
                web.set_html_id("node_info", state.get_node_name());
                web.set_html_id("version", state.get_version());
                web.set_html_id("messages", state.get_msgs());
                web.set_html_id("nodes_online", format!("{}", state.nodes_online));
                web.set_html_id("nodes_connected", format!("{}", state.nodes_connected));
                web.set_html_id("mana", format!("{}", state.mana));
                web.set_html_id("msgs_system", format!("{}", state.msgs_system));
                web.set_html_id("msgs_local", format!("{}", state.msgs_local));
            }
            wait_ms(1000).await;
        }
    });
}

fn update_table(web: &FledgerWeb, state: &FledgerState) -> Result<(), JsValue> {
    let stats_table = state.get_node_table();
    let el_fetching = web.document.get_element_by_id("fetching").unwrap();
    let el_table_stats = web.document.get_element_by_id("table_stats").unwrap();
    el_table_stats.class_list().remove_1("hidden")?;
    el_fetching.class_list().remove_1("hidden")?;
    if stats_table == "" {
        el_table_stats.class_list().add_1("hidden")?;
    } else {
        el_fetching.class_list().add_1("hidden")?;
        web.set_html_id("node_stats", stats_table);
    }

    Ok(())
}

/// FledgerWeb is a nearly generic handler for the FledgerNode.
pub struct FledgerWeb {
    node: Node,
    document: Document,
    counter: u32,
}

impl FledgerWeb {
    pub async fn new() -> Result<Self> {
        console_error_panic_hook::set_once();
        FledgerWeb::set_data_storage();

        wasm_logger::init(wasm_logger::Config::new(log::Level::Debug));
        log::info!("Starting new FledgerWeb on {URL}");

        // Get a link to the document
        let window = web_sys::window().expect("no global `window` exists");
        Ok(Self {
            node: Self::node_start().await?,
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
        self.document
            .get_element_by_id(id)
            .unwrap()
            .add_event_listener_with_callback("click", &cb.as_ref().unchecked_ref())
            .expect("Should be able to add event listener");
    }

    fn set_html_id(&self, id: &str, inner_html: String) {
        self.document
            .get_element_by_id(id)
            .unwrap()
            .set_inner_html(&inner_html);
    }

    fn get_element<ET: JsCast>(&self, id: &str) -> ET {
        self.document
            .get_element_by_id(id)
            .unwrap()
            .dyn_into::<ET>()
            .unwrap()
    }

    pub async fn tick(&mut self) -> Option<FledgerState> {
        let mut fs = None;
        match FledgerState::new(&self.node) {
            Ok(f) => fs = Some(f),
            Err(e) => log::error!("Couldn't create state: {:?}", e),
        }
        self.counter += 1;
        self.node
            .request_list()
            .await
            .expect("Couldn't request list");
        self.node
            .process()
            .await
            .expect("Should be able to push process");
        fs
    }

    async fn node_start() -> Result<Node> {
        let my_storage = DataStorageLocal::new("fledger");
        let mut node_config = Node::get_config(my_storage.clone())?;
        let config = ConnectionConfig::new(
            Some(URL.into()),
            None,
            Some(HostLogin {
                url: "turn:web.fledg.re:3478".into(),
                login: Some(Login {
                    user: "something".into(),
                    pass: "something".into(),
                }),
            }),
        );
        let network = network_broker_start(node_config.clone(), config).await?;
        node_config.info.modules = Modules::all() - Modules::ENABLE_WEBPROXY_REQUESTS;
        Ok(Node::start(my_storage, node_config, network, None)
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
    pub msgs_system: usize,
    pub msgs_local: usize,
    pub mana: u32,
    pub nodes_online: usize,
    pub nodes_connected: usize,
}

impl FledgerState {
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
}

struct NodeDesc {
    info: String,
    ping: PingStat,
    stat: String,
}

impl FledgerState {
    fn new(node: &Node) -> Result<Self> {
        let info = node.node_config.info.clone();
        let msgs = node.gossip.as_ref().unwrap().chat_events();
        let nodes_info = node.nodes_info_all()?;
        Ok(Self {
            info,
            nodes_online: node.nodes_online()?.len(),
            nodes_connected: node.nodes_connected()?.len(),
            msgs_system: 0,
            msgs_local: msgs.len(),
            mana: 0,
            msgs: FledgerMessages::new(msgs, &nodes_info.clone().into_values().collect()),
            nodes_info,
            states: node.stat.as_ref().unwrap().states.clone(),
            pings: node.ping.as_ref().unwrap().storage.clone(),
        })
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

#[derive(Clone)]
pub struct FledgerMessage {
    from: String,
    date: String,
    text: String,
}

#[derive(Clone)]
pub struct FledgerMessages {
    msgs: Vec<FledgerMessage>,
}

impl FledgerMessages {
    fn new(
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
                from,
                text: msg.msg.clone(),
                date,
            })
        }
        FledgerMessages { msgs }
    }
}

impl FledgerMessages {
    pub fn get_messages(&self) -> String {
        if self.msgs.is_empty() {
            return String::from("No messages");
        }
        format!(
            "<ul><li>{}</li></ul>",
            self.msgs
                .iter()
                .map(|fm| format!(
                    "{} wrote on {}:<br><pre>{}</pre>",
                    fm.from.clone(),
                    fm.date.clone(),
                    fm.text.clone()
                ))
                .collect::<Vec<String>>()
                .join("</li><li>")
        )
    }
}
