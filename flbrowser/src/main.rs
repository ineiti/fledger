use anyhow::{anyhow, Result};
use chrono::{prelude::DateTime, Utc};
use flmodules::ping::storage::{PingStat, PingStorage};
use flnet::{
    config::{ConnectionConfig, HostLogin, Login},
    network_broker_start,
};
use flnode::{node::Node, version::VERSION_STRING};
use regex::Regex;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, UNIX_EPOCH},
};
use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
use web_sys::{window, Document, Event, HtmlTextAreaElement};

use flarch::{data_storage::DataStorageLocal, spawn_local, wait_ms};
use flmodules::nodeids::U256;
use flnet::{config::NodeInfo, network::NetworkConnectionState};

#[cfg(not(feature = "local"))]
const URL: &str = "wss://signal.fledg.re";

#[cfg(feature = "local")]
const URL: &str = "ws://localhost:8765";

fn main() {
    spawn_local(async {
        let mut web = FledgerWeb::new();
        let window = web_sys::window().expect("no global `window` exists");
        let document = window.document().expect("should have a document on window");
        let your_message = document
            .get_element_by_id("your_message")
            .unwrap()
            .dyn_into::<HtmlTextAreaElement>()
            .unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();

        let cb = Closure::wrap(Box::new(move |_: Event| {
            tx.send(())
                .err()
                .map(|e| log::error!("Couldn't send message: {e:?}"));
        }) as Box<dyn FnMut(_)>);
        document
            .get_element_by_id("send_msg")
            .unwrap()
            .add_event_listener_with_callback("click", &cb.as_ref().unchecked_ref())
            .expect("Should be able to add event listener");

        loop {
            if let Ok(_) = rx.try_recv() {
                let msg = your_message.value();
                if msg != "" {
                    web.send_msg(msg);
                    your_message.set_value("");
                }
            }
            if let Some(state) = web.tick() {
                update_table(&document, &state)
                    .err()
                    .map(|_| log::error!("While updating table"));
                sh(&document, "node_info", state.get_node_name());
                sh(&document, "version", state.get_version());
                sh(&document, "messages", state.get_msgs());
                sh(&document, "nodes_known", format!("{}", state.nodes_known));
                sh(&document, "nodes_online", format!("{}", state.nodes_online));
                sh(
                    &document,
                    "nodes_connected",
                    format!("{}", state.nodes_connected),
                );
            }
            wait_ms(1000).await;
        }
    });
}

fn sh(doc: &Document, id: &str, inner_html: String) {
    doc.get_element_by_id(id)
        .unwrap()
        .set_inner_html(&inner_html);
}

fn update_table(document: &Document, state: &FledgerState) -> Result<(), JsValue> {
    let stats_table = state.get_node_table();
    let el_fetching = document.get_element_by_id("fetching").unwrap();
    let el_table_stats = document.get_element_by_id("table_stats").unwrap();
    el_table_stats.class_list().remove_1("hidden")?;
    el_fetching.class_list().remove_1("hidden")?;
    if stats_table == "" {
        el_table_stats.class_list().add_1("hidden")?;
    } else {
        el_fetching.class_list().add_1("hidden")?;
        sh(&document, "node_stats", stats_table);
    }

    Ok(())
}

pub struct FledgerWeb {
    node: Arc<Mutex<Option<Node>>>,
    counter: u32,
    msgs: Vec<String>,
}

impl FledgerWeb {
    pub fn new() -> Self {
        console_error_panic_hook::set_once();
        FledgerWeb::set_data_storage();

        wasm_logger::init(wasm_logger::Config::new(log::Level::Debug));
        log::info!("Starting new FledgerWeb");

        let fw = Self {
            node: Arc::new(Mutex::new(None)),
            counter: 0u32,
            msgs: vec![],
        };

        let node_cl = fw.node.clone();
        wasm_bindgen_futures::spawn_local(async {
            match FledgerWeb::node_start(node_cl).await {
                Ok(_) => log::info!("Initialized node"),
                Err(e) => log::error!("Couldn't initialize node: {}", e),
            }
        });
        fw
    }

    pub fn tick(&mut self) -> Option<FledgerState> {
        let mut fs = None;
        if let Ok(mut no) = self.node.try_lock() {
            if let Some(n) = no.as_mut() {
                match FledgerState::new(n) {
                    Ok(f) => fs = Some(f),
                    Err(e) => log::error!("Couldn't create state: {:?}", e),
                }
            }
        } else {
            log::error!("Couldn't lock");
        }
        let noc = Arc::clone(&self.node);
        let msgs = self.msgs.drain(..).collect();
        self.counter += 1;
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = Self::update_node(noc, msgs).await {
                log::error!("Couldn't update node: {:?}", e);
            };
        });
        fs
    }

    pub fn send_msg(&mut self, msg: String) {
        self.msgs.push(msg);
    }
}

impl Default for FledgerWeb {
    fn default() -> Self {
        Self::new()
    }
}

impl FledgerWeb {
    async fn update_node(noc: Arc<Mutex<Option<Node>>>, msgs: Vec<String>) -> Result<()> {
        if let Ok(mut no) = noc.try_lock() {
            if let Some(n) = no.as_mut() {
                for msg in msgs {
                    if let Err(e) = n.add_chat_message(msg).await {
                        log::error!("Couldn't add message: {:?}", e);
                    }
                }
                n.request_list().await.map_err(|e| anyhow!(e))?;
                n.process().await.map_err(|e| anyhow!(e))?;
            } else {
                log::warn!("Couldn't lock node");
            }
        } else {
            log::error!("Couldn't lock");
        }
        Ok(())
    }

    async fn node_start(node_mutex: Arc<Mutex<Option<Node>>>) -> Result<()> {
        let my_storage = DataStorageLocal::new("fledger");
        if let Ok(mut node) = node_mutex.try_lock() {
            let node_config = Node::get_config(my_storage.clone())?;
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
            let network =
                network_broker_start(node_config.clone(), config)
                    .await?;
            *node = Some(
                Node::start(my_storage, node_config, network)
                    .await
                    .map_err(|e| anyhow!("Couldn't create node: {:?}", e))?,
            );
        } else {
            log::error!("Couldn't lock");
        }
        Ok(())
    }

    fn set_config(data: &str) {
        if let Err(err) = Node::set_config(DataStorageLocal::new("fledger"), data) {
            log::warn!("Got error while saving config: {}", err);
        }
    }

    fn set_data_storage() {
        if let Ok(loc) = window().unwrap().location().href() {
            if loc.contains('#') {
                let reg = Regex::new(r".*?#").unwrap();
                let data_enc = reg.replace(&loc, "");
                if data_enc != "" {
                    if let Ok(data) = urlencoding::decode(&data_enc) {
                        FledgerWeb::set_config(&data);
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
    pub msgs_system: u32,
    pub msgs_local: u32,
    pub mana: u32,
    pub nodes_known: usize,
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
            nodes_known: nodes_info.len(),
            nodes_online: node.nodes_online()?.len(),
            nodes_connected: node.nodes_connected()?.len(),
            msgs: FledgerMessages::new(msgs, &nodes_info.clone().into_values().collect()),
            nodes_info,
            msgs_system: 0,
            msgs_local: 0,
            mana: 0,
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
        mut tm_msgs: Vec<flmodules::gossip_events::events::Event>,
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
