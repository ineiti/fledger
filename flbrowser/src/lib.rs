use anyhow::{anyhow, Result};
use chrono::{prelude::DateTime, Utc};
use flmodules::ping::storage::{PingStat, PingStorage};
use flnet_wasm::{web_rtc::web_rtc_spawner, web_socket_client::WebSocketClient};
use flnode::{node::Node, node_data::NodeData, version::VERSION_STRING};
use regex::Regex;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, UNIX_EPOCH},
};
use wasm_bindgen::prelude::*;
use web_sys::window;

use flnet::{config::NodeInfo, network::NetworkConnectionState};
use flarch::{
    data_storage::{DataStorageBase, DataStorageBaseImpl},
    nodeids::U256,
};

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
                n.list().await.map_err(|e| anyhow!(e))?;
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
        let rtc_spawner = web_rtc_spawner();
        let my_storage = Box::new(DataStorageBaseImpl {});
        let ws = WebSocketClient::connect(URL)
            .await
            .map_err(|e| anyhow!("couldn't create websocket: {:?}", e))?;
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
            let node_config = NodeData::get_config(my_storage.clone())?;
            *node = Some(
                Node::new(my_storage, node_config, &client, ws, rtc_spawner)
                    .await
                    .map_err(|e| anyhow!("Couldn't create node: {:?}", e))?,
            );
        } else {
            log::error!("Couldn't lock");
        }
        Ok(())
    }

    fn set_config(data: &str) {
        if let Err(err) = Node::set_config(DataStorageBaseImpl {}.get("fledger"), data) {
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

#[wasm_bindgen]
pub struct FledgerState {
    info: NodeInfo,
    nodes_info: HashMap<U256, NodeInfo>,
    stats: HashMap<U256, NetworkConnectionState>,
    pings: PingStorage,
    msgs: FledgerMessages,
    pub msgs_system: u32,
    pub msgs_local: u32,
    pub mana: u32,
    pub nodes_online: usize,
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

struct NodeDesc {
    info: String,
    ping: PingStat,
    stat: String,
}

impl FledgerState {
    fn new(node: &Node) -> Result<Self> {
        let info = node.info();
        let msgs = node.get_chat_messages();
        let nodes_info = node.nodes_info_all()?;
        Ok(Self {
            info,
            nodes_online: nodes_info.len(),
            msgs: FledgerMessages::new(msgs, &nodes_info.clone().into_values().collect()),
            nodes_info,
            msgs_system: 0,
            msgs_local: 0,
            mana: 0,
            stats: node.nodes_stat(),
            pings: node.nodes_ping(),
        })
    }

    fn get_nodes(&self) -> Vec<NodeDesc> {
        let mut out = vec![];
        let mut nodes: Vec<(&U256, &NodeInfo)> =
            self.nodes_info.iter().map(|(k, v)| (k, v)).collect();
        nodes.sort_by(|a, b| a.1.info.partial_cmp(&b.1.info).unwrap());
        for (id, ni) in &nodes {
            let stat = self
                .stats
                .get(id)
                .map(|s| format!("{:?}", s.s.type_local))
                .unwrap_or("n/a".into());
            let info = ni.info.clone();
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
                    node.ping.countdown.to_string(),
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
