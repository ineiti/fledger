use anyhow::{anyhow, Result};
use chrono::{prelude::DateTime, Utc};
use flnet::config::NodeInfo;
use js_sys::Date;
use regex::Regex;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, UNIX_EPOCH},
};
use wasm_bindgen::prelude::*;
use wasm_webrtc::helpers::LocalStorageBase;
use wasm_webrtc::{web_rtc_setup::WebRTCConnectionSetupWasm, web_socket::WebSocketWasm};
use web_sys::window;

use flnode::node::{stats::StatNode, version::VERSION_STRING, Node};

use flutils::data_storage::DataStorageBase;

#[cfg(not(feature = "local"))]
const URL: &str = "wss://signal.fledg.re";

#[cfg(feature = "local")]
const URL: &str = "ws://localhost:8765";

#[wasm_bindgen]
pub struct FledgerWeb {
    node: Arc<Mutex<Option<Node>>>,
    counter: u32,
    msgs: Vec<String>,
}

#[wasm_bindgen]
impl FledgerWeb {
    pub fn new() -> Self {
        console_error_panic_hook::set_once();
        FledgerWeb::set_localstorage();

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
                if !self.msgs.is_empty() {
                    let msg = self.msgs.remove(0);
                    if let Err(e) = n.add_chat_message(msg) {
                        log::error!("Couldn't add message: {:?}", e);
                    }
                }
                match FledgerState::new(n) {
                    Ok(f) => fs = Some(f),
                    Err(e) => log::error!("Couldn't create state: {:?}", e),
                }
            }
        } else {
            log::error!("Couldn't lock");
        }
        let noc = Arc::clone(&self.node);
        self.counter += 1;
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = Self::update_node(noc).await {
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
    async fn update_node(noc: Arc<Mutex<Option<Node>>>) -> Result<()> {
        if let Ok(mut no) = noc.try_lock() {
            if let Some(n) = no.as_mut() {
                n.list().map_err(|e| anyhow!(e))?;
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
        let rtc_spawner = Box::new(WebRTCConnectionSetupWasm::new_box);
        let my_storage = Box::new(LocalStorageBase {});
        let ws =
            WebSocketWasm::new(URL).map_err(|e| anyhow!("couldn't create websocket: {:?}", e))?;
        let client = if let Some(window) = web_sys::window() {
            let navigator = window.navigator();
            match navigator.user_agent() {
                Ok(p) => p,
                Err(_) => "n/a".to_string(),
            }
        } else {
            "node".to_string()
        };
        if let Ok(mut node) = node_mutex.try_lock() {
            *node = Some(
                Node::new(my_storage, &client, Box::new(ws), rtc_spawner)
                    .map_err(|e| anyhow!("Couldn't create node: {:?}", e))?,
            );
        } else {
            log::error!("Couldn't lock");
        }
        Ok(())
    }

    fn set_config(data: &str) {
        if let Err(err) = Node::set_config(LocalStorageBase {}.get("fledger"), data) {
            log::warn!("Got error while saving config: {}", err);
        }
    }

    fn set_localstorage() {
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

#[wasm_bindgen]
pub struct FledgerState {
    info: NodeInfo,
    stats: Vec<StatNode>,
    msgs: FledgerMessages,
    pub nodes_online: u32,
    pub msgs_system: u32,
    pub msgs_local: u32,
    pub mana: u32,
}

#[wasm_bindgen]
impl FledgerState {
    pub fn get_node_name(&self) -> String {
        self.info.info.clone()
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

impl FledgerState {
    fn new(node: &Node) -> Result<Self> {
        let info = node.info().map_err(|e| anyhow!(e))?;
        let stats: Vec<StatNode> = node
            .stats()
            .map_err(|e| anyhow!(e))?
            .iter()
            .map(|(_k, v)| v.clone())
            .collect();
        let msgs = node.get_chat_messages().map_err(|e| anyhow!(e))?;
        Ok(Self {
            info,
            msgs: FledgerMessages::new(msgs, node.get_list_nodes()?),
            nodes_online: stats.len() as u32,
            msgs_system: 0,
            msgs_local: 0,
            mana: 0,
            stats,
        })
    }

    fn get_node_table_result(&self) -> Result<String> {
        let mut node_stats = self.stats.clone();
        node_stats.sort_by(|a, b| b.last_contact.partial_cmp(&a.last_contact).unwrap());
        let now = Date::now();
        let mut stats_vec = vec![];
        for stat in node_stats {
            if let Some(ni) = stat.node_info.as_ref() {
                if self.info.get_id() != ni.get_id() {
                    stats_vec.push(
                        vec![
                            ni.info.to_string(),
                            format!("rx:{} tx:{}", stat.ping_rx, stat.ping_tx),
                            format!("{}s", ((now - stat.last_contact) / 1000.).floor()),
                            format!("in:{:?} out:{:?}", stat.incoming, stat.outgoing),
                        ]
                        .join("</td><td>"),
                    );
                }
            }
        }
        Ok(format!(
            "<tr><td>{}</td></tr>",
            stats_vec.join("</td></tr><tr><td>")
        ))
    }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct FledgerMessage {
    from: String,
    date: String,
    text: String,
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct FledgerMessages {
    msgs: Vec<FledgerMessage>,
}

impl FledgerMessages {
    fn new(mut tm_msgs: Vec<flmodules::gossip_events::events::Event>, nodes: Vec<NodeInfo>) -> Self {
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
                node[0].info.clone()
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

#[wasm_bindgen]
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
