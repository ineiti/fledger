pub mod config;
pub mod ext_interface;
pub mod logic;
pub mod network;
pub mod types;

use crate::node::{
    config::{NodeConfig, NodeInfo},
    ext_interface::{DataStorage, Logger},
    logic::Logic,
    network::{NOutput, Network},
    types::U256,
};
use crate::signal::{web_rtc::WebRTCSpawner, websocket::WebSocketConnection};

use self::{
    logic::{LInput, LOutput},
    network::NInput,
};

/// The node structure holds it all together. It is the main structure of the project.
pub struct Node {
    pub network: Network,
    pub info: NodeInfo,
    pub logic: Logic,
    _storage: Box<dyn DataStorage>,
    logger: Box<dyn Logger>,
}

pub const CONFIG_NAME: &str = "nodeConfig";

impl Node {
    /// Create new node by loading the config from the storage.
    /// This also initializes the network and starts listening for
    /// new messages from the signalling server and from other nodes.
    /// The actual logic is handled in Logic.
    pub fn new(
        _storage: Box<dyn DataStorage>,
        logger: Box<dyn Logger>,
        ws: Box<dyn WebSocketConnection>,
        web_rtc: WebRTCSpawner,
    ) -> Result<Node, String> {
        let config_str = match _storage.load(CONFIG_NAME) {
            Ok(s) => s,
            Err(_) => {
                logger.info(&format!("Couldn't load configuration - start with empty"));
                "".to_string()
            }
        };
        let config = NodeConfig::new(config_str)?;
        _storage.save(CONFIG_NAME, &config.to_string()?)?;
        logger.info(&format!(
            "Starting node: {} = {}",
            config.our_node.info, config.our_node.public
        ));
        let network = Network::new(logger.clone(), config.our_node.clone(), ws, web_rtc);
        let logic = Logic::new(config.our_node.clone(), logger.clone());

        Ok(Node {
            info: config.our_node,
            _storage,
            network,
            logger,
            logic,
        })
    }

    pub async fn process(&mut self) -> Result<(), String> {
        self.process_logic()?;
        self.process_network()?;
        self.logic.process().await?;
        self.network.process().await?;
        Ok(())
    }

    fn process_network(&mut self) -> Result<(), String> {
        let msgs: Vec<NOutput> = self.network.output_rx.try_iter().collect();
        for msg in msgs {
            match msg {
                NOutput::WebRTC(id, msg) => {
                    // self.logger.info(&format!(
                    //     "dbg: Node::process webrtc message {} from {}",
                    //     msg, id
                    // ));
                    self.logic
                        .input_tx
                        .send(LInput::WebRTC(id, msg))
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(())
    }

    fn process_logic(&mut self) -> Result<(), String> {
        let msgs: Vec<LOutput> = self.logic.output_rx.try_iter().collect();
        for msg in msgs {
            match msg {
                logic::LOutput::WebRTC(id, msg) => self
                    .network
                    .input_tx
                    .send(NInput::WebRTC(id, msg))
                    .map_err(|e| e.to_string())?,
            }
        }
        Ok(())
    }

    /// TODO: this is only for development
    pub fn clear(&mut self) -> Result<(), String> {
        self.network.clear_nodes()
    }

    /// Requests a list of all connected nodes
    pub fn list(&mut self) -> Result<(), String> {
        self.network.update_node_list()
    }

    /// Gets the current list
    pub fn get_list(&mut self) -> Vec<NodeInfo> {
        self.network.get_list()
    }

    /// Get number of pings from each remote node
    pub fn get_pings(&self) -> Result<Vec<(NodeInfo, u64)>, String> {
        let pings = self.logic.pings.clone();
        return Ok(self
            .network
            .get_list()
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
        for node in &self.network.get_list() {
            self.logger.info(&format!("Contacting node {:?}", node));
            let ping = format!("Ping {}", msg);
            self.send(&node.public, ping)?;
        }
        Ok(())
    }

    /// Sends a message over webrtc to a node. The node must already be connected
    /// through websocket to the signalling server. If the connection is not set up
    /// yet, the network stack will set up a connection with the remote node.
    pub fn send(&mut self, dst: &U256, msg: String) -> Result<(), String> {
        self.network
            .input_tx
            .send(NInput::WebRTC(dst.clone(), msg))
            .map_err(|e| e.to_string())
    }

    pub fn set_config(storage: Box<dyn DataStorage>, config: &str) -> Result<(), String> {
        storage.save(CONFIG_NAME, config)
    }
}
