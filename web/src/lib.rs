use anyhow::{anyhow, Result};
use js_sys::Date;
use log::{debug, error, info, warn};
use regex::Regex;
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;
use wasm_webrtc::{
    helpers::LocalStorage, web_rtc_setup::WebRTCConnectionSetupWasm, web_socket::WebSocketWasm,
};
use web_sys::window;

use common::node::{
    config::NodeInfo,
    logic::{stats::StatNode, text_messages::TextMessage},
    version::VERSION_STRING,
    Node,
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
        FledgerWeb::set_localstorage();

        wasm_logger::init(wasm_logger::Config::default());
        info!("Starting new FledgerWeb");

        let fw = Self {
            node: Arc::new(Mutex::new(None)),
            counter: 0u32,
            msgs: vec![],
        };

        let node_cl = fw.node.clone();
        wasm_bindgen_futures::spawn_local(async {
            match FledgerWeb::node_start(node_cl).await {
                Ok(_) => info!("Initialized node"),
                Err(e) => error!("Couldn't initialize node: {}", e),
            }
        });
        fw
    }

    pub fn tick(&mut self) -> Option<FledgerState> {
        let mut fs = None;
        if let Ok(mut no) = self.node.try_lock() {
            if let Some(n) = no.as_mut() {
                if self.msgs.len() > 0 {
                    let msg = self.msgs.remove(0);
                    debug!("Sending message {}", msg);
                    if let Err(e) = n.add_message(msg) {
                        error!("Couldn't add message: {:?}", e);
                    }
                }
                match FledgerState::new(n) {
                    Ok(f) => fs = Some(f),
                    Err(e) => error!("Couldn't create state: {:?}", e),
                }
            }
        }
        let noc = Arc::clone(&self.node);
        self.counter += 1;
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = Self::update_node(noc).await {
                error!("Couldn't update node: {:?}", e);
            };
        });
        fs
    }

    pub fn send_msg(&mut self, msg: String) {
        self.msgs.push(msg);
    }
}

impl FledgerWeb {
    async fn update_node(noc: Arc<Mutex<Option<Node>>>) -> Result<()> {
        if let Ok(mut no) = noc.try_lock() {
            if let Some(n) = no.as_mut() {
                n.list().map_err(|e| anyhow!(e))?;
                n.process().await.map_err(|e| anyhow!(e))?;
            } else {
                warn!("Couldn't lock node");
            }
        };
        Ok(())
    }

    async fn node_start(node_mutex: Arc<Mutex<Option<Node>>>) -> Result<()> {
        let rtc_spawner = Box::new(|cs| WebRTCConnectionSetupWasm::new(cs));
        let my_storage = Box::new(LocalStorage {});
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
        }
        Ok(())
    }

    fn set_config(data: &str) {
        if let Err(err) = Node::set_config(Box::new(LocalStorage {}), &data) {
            info!("Got error while saving config: {}", err);
        }
    }

    fn set_localstorage() {
        if let Ok(loc) = window().unwrap().location().href() {
            info!("Location is: {}", loc.clone());
            if loc.contains("#") {
                let reg = Regex::new(r".*?#").unwrap();
                let data_enc = reg.replace(&loc, "");
                if data_enc != "" {
                    info!("Setting data");
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
        let msgs = node.get_messages().map_err(|e| anyhow!(e))?;
        Ok(Self {
            info,
            msgs: FledgerMessages::new(msgs),
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
                            format!("{}", ni.info),
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
    text: String,
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct FledgerMessages {
    msgs: Vec<FledgerMessage>,
}

impl FledgerMessages {
    fn new(mut msgs: Vec<TextMessage>) -> Self {
        msgs.sort_by(|a, b| b.created.partial_cmp(&a.created).unwrap());
        FledgerMessages {
            msgs: msgs
                .iter()
                .map(|msg| FledgerMessage {
                    from: msg.node_info.clone(),
                    text: msg.msg.clone(),
                })
                .collect(),
        }
    }
}

#[wasm_bindgen]
impl FledgerMessages {
    pub fn get_messages(&self) -> String {
        if self.msgs.len() == 0 {
            return String::from("No messages");
        }
        format!(
            "<ul><li>{}</li></ul>",
            self.msgs
                .iter()
                .map(|fm| format!("{}: {}", fm.from.clone(), fm.text.clone()))
                .collect::<Vec<String>>()
                .join("</li><li>")
        )
    }
}
