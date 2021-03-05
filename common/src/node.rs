pub mod config;
pub mod ext_interface;
pub mod logic;
pub mod network;
pub mod types;

use crate::node::{
    config::{NodeConfig, NodeInfo},
    ext_interface::{DataStorage, Logger},
    logic::Logic,
    network::{Network, WebRTCReceive},
    types::U256,
};
use crate::signal::{web_rtc::WebRTCSpawner, websocket::WebSocketConnection};
use std::sync::{Arc, Mutex};

// use self::logic::Logic;
// mod logic;

/// The node structure holds it all together. It is the main structure of the project.
pub struct Node {
    pub info: NodeInfo,
    pub nodes: Vec<NodeInfo>,
    pub network: Network,
    pub storage: Box<dyn DataStorage>,
    pub logger: Box<dyn Logger>,
    logic: Arc<Mutex<Logic>>,
}

pub const CONFIG_NAME: &str = "nodeConfig";

impl Node {
    /// Create new node by loading the config from the storage.
    /// This also initializes the network and starts listening for
    /// new messages from the signalling server and from other nodes.
    /// The actual logic is handled in Logic.
    pub fn new(
        storage: Box<dyn DataStorage>,
        logger: Box<dyn Logger>,
        ws: Box<dyn WebSocketConnection>,
        web_rtc: WebRTCSpawner,
    ) -> Result<Node, String> {
        let config_str = match storage.load(CONFIG_NAME) {
            Ok(s) => s,
            Err(_) => {
                logger.info(&format!("Couldn't load configuration - start with empty"));
                "".to_string()
            }
        };
        let config = NodeConfig::new(config_str)?;
        storage.save(CONFIG_NAME, &config.to_string()?)?;
        logger.info(&format!(
            "Starting node: {} = {}",
            config.our_node.info, config.our_node.public
        ));
        let logic = Logic::new(config.our_node.clone(), logger.clone());
        let logic_clone = Arc::clone(&logic);
        let web_rtc_rcv: WebRTCReceive = Arc::new(Mutex::new(Box::new(move |id, msg| {
            logic_clone.lock().unwrap().rcv(id, msg);
        })));
        let network = Network::new(
            ws,
            web_rtc,
            web_rtc_rcv,
            logger.clone(),
            config.our_node.clone(),
        );

        Ok(Node {
            info: config.our_node,
            storage,
            network,
            logger,
            nodes: vec![],
            logic,
        })
    }

    /// TODO: this is only for development
    pub async fn clear(&self) -> Result<(), String> {
        self.network.clear_nodes()
    }

    /// Requests a list of all connected nodes
    pub fn list(&mut self) -> Result<(), String> {
        self.network.update_node_list()
    }

    /// Gets the current list
    pub fn get_list(&mut self) -> Result<Vec<NodeInfo>, String> {
        self.network.get_list()
    }

    /// Get number of pings from each remote node
    pub fn get_pings(&self) -> Result<Vec<(NodeInfo, u64)>, String> {
        let pings = self.logic.lock().unwrap().pings.clone();
        return Ok(self
            .network
            .get_list()?
            .iter()
            .map(|n| (n.clone(), *pings.get(&n.public).or(Some(&0u64)).unwrap()))
            .collect());
    }

    /// Returns a nicely formatted string of the nodes with number of pings received
    pub fn get_pings_str(&self) -> Result<String, String> {
        let lg = self.get_pings()?;
        if lg.len() > 0 {
            return Ok(lg
                .iter()
                .map(|ni| format!("({} / {})", ni.0.info.clone(), ni.1))
                .collect::<Vec<String>>()
                .join(" :: "));
        }
        Ok("".into())
    }

    /// Pings all known nodes
    pub async fn ping(&mut self, msg: &str) -> Result<(), String> {
        for node in &self.network.get_list()? {
            self.logger.info(&format!("Contacting node {:?}", node));
            let ping = format!("Ping {}", msg);
            match self.network.send(&node.public, ping).await {
                Ok(_) => self.logger.info("Successfully sent ping"),
                Err(e) => self
                    .logger
                    .error(&format!("Error while sending ping: {:?}", e)),
            }
        }
        Ok(())
    }

    pub async fn send(&self, dst: &U256, msg: String) -> Result<(), String> {
        self.network.send(dst, msg).await
    }

    pub fn set_config(storage: Box<dyn DataStorage>, config: &str) -> Result<(), String> {
        storage.save(CONFIG_NAME, config)
    }
}
