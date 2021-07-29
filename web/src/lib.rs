use anyhow::{anyhow, Result};
use js_sys::Date;
use log::{error, info, warn};
use regex::Regex;
use std::{
    collections::HashMap,
    convert::TryInto,
    sync::{Arc, Mutex},
};
use wasm_bindgen::prelude::*;
use wasm_webrtc::{
    helpers::LocalStorage, web_rtc_setup::WebRTCConnectionSetupWasm, web_socket::WebSocketWasm,
};
use web_sys::window;

use common::{
    node::{config::NodeInfo, logic::stats::statnode::StatNode, version::VERSION_STRING, Node},
    types::U256,
};

#[cfg(not(feature = "local"))]
const URL: &str = "wss://signal.fledg.re";

#[cfg(feature = "local")]
const URL: &str = "ws://localhost:8765";

#[wasm_bindgen]
pub struct FledgerWeb {
    node: Arc<Mutex<Option<Node>>>,
    counter: u32,
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

    pub fn tick(&mut self) -> FledgerState {
        let mut fs = FledgerState::empty();
        if let Ok(mut no) = self.node.try_lock() {
            if let Some(n) = no.as_mut() {
                match FledgerState::new(n) {
                    Ok(f) => fs = f,
                    Err(e) => error!("Couldn't create state: {:?}", e),
                }
            }
        }
        let noc = Arc::clone(&self.node);
        self.counter += 1;
        let ping = self.counter & 3 == 0;
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = Self::update_node(noc, ping).await {
                error!("Couldn't update node: {:?}", e);
            };
        });
        fs
    }
}

impl FledgerWeb {
    async fn update_node(noc: Arc<Mutex<Option<Node>>>, ping: bool) -> Result<()> {
        if let Ok(mut no) = noc.try_lock() {
            if let Some(n) = no.as_mut() {
                n.list().map_err(|e| anyhow!(e))?;
                if ping {
                    n.ping().await.map_err(|e| anyhow!(e))?;
                }
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
                    .map_err(|e| anyhow!("Couldn't create node: {:?}", e.as_str()))?,
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
    stats: HashMap<U256, StatNode>,
}

#[wasm_bindgen]
impl FledgerState {
    pub fn get_node_name(&self) -> String {
        self.info.info.clone()
    }

    pub fn get_stats_table(&self) -> String {
        match self.get_stats_table_result() {
            Ok(str) => str,
            Err(e) => {
                error!("Couldn't get result: {:?}", e);
                format!("")
            }
        }
    }

    fn get_stats_table_result(&self) -> Result<String> {
        let mut stats_node: Vec<StatNode> = self.stats.iter().map(|(_k, v)| v.clone()).collect();
        stats_node.sort_by(|a, b| b.last_contact.partial_cmp(&a.last_contact).unwrap());
        let now = Date::now();
        let mut stats_vec = vec![];
        for stat in stats_node {
            if let Some(ni) = stat.node_info.as_ref() {
                if self.info.id != ni.id {
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

    pub fn get_version(&self) -> String {
        VERSION_STRING.to_string()
    }

    pub fn get_nodes_online(&self) -> u32 {
        self.stats.len().try_into().unwrap_or(0)
    }

    pub fn get_msgs_system(&self) -> u32 {
        0
    }

    pub fn get_msgs_local(&self) -> u32 {
        0
    }

    pub fn get_msgs(&self) -> FledgerMessages{
        FledgerMessages::new()
    }

    pub fn get_mana(&self) -> u32 {
        0
    }
}

impl FledgerState {
    fn new(node: &Node) -> Result<Self> {
        Ok(Self {
            info: node.info().unwrap_or(NodeInfo::new()),
            stats: node.stats().map_err(|e| anyhow!(e))?,
        })
    }

    fn empty() -> Self {
        Self {
            info: NodeInfo::new(),
            stats: HashMap::new(),
        }
    }
}

#[wasm_bindgen]
pub struct FledgerMessage {
    pub text: String,
}

#[wasm_bindgen]
pub struct FledgerMessages {
    pub msgs: Vec<FledgerMessage>,
}

impl FledgerMessages {
    fn new() -> Self {
        &FledgerMessages{
            msgs: vec![]
        }
    }
}
