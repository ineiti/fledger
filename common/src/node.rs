use std::{
    collections::HashMap,
    pin::Pin,
    sync::{mpsc::Sender, Arc, Mutex},
};

use futures::Future;
use log::{error, info, trace};

use crate::node::{
    config::{NodeConfig, NodeInfo},
    logic::Logic,
    network::{NOutput, Network},
};
use crate::signal::{web_rtc::WebRTCSpawner, websocket::WebSocketConnection};
use crate::types::{DataStorage, U256};

use self::{
    logic::{LInput, LOutput, Stat},
    network::NInput,
};

pub mod config;
pub mod logic;
pub mod network;
pub mod version;

/// The node structure holds it all together. It is the main structure of the project.
pub struct Node {
    arc: Arc<Mutex<NodeArc>>,
    config: NodeConfig,
    network_tx: Sender<NInput>,
    logic_tx: Sender<LInput>,
}

struct NodeArc {
    network: Network,
    logic: Logic,
}

pub const CONFIG_NAME: &str = "nodeConfig";

#[cfg(target_arch = "wasm32")]
fn spawn_block(f: Pin<Box<dyn Future<Output = ()>>>) {
    wasm_bindgen_futures::spawn_local(f);
}

#[cfg(not(target_arch = "wasm32"))]
// fn spawn_block(f: dyn std::future::Future){
// fn spawn_block(f: Box<dyn FnMut() -> Box<dyn Future<Output=()>>>){
fn spawn_block(f: Pin<Box<dyn Future<Output = ()>>>) {
    futures::executor::block_on(f);
}

impl Node {
    /// Create new node by loading the config from the storage.
    /// This also initializes the network and starts listening for
    /// new messages from the signalling server and from other nodes.
    /// The actual logic is handled in Logic.
    pub fn new(
        storage: Box<dyn DataStorage>,
        client: &str,
        ws: Box<dyn WebSocketConnection>,
        web_rtc: WebRTCSpawner,
    ) -> Result<Node, String> {
        let config_str = match storage.load(CONFIG_NAME) {
            Ok(s) => s,
            Err(_) => {
                info!("Couldn't load configuration - start with empty");
                "".to_string()
            }
        };
        let mut config = NodeConfig::new(config_str)?;
        config.our_node.client = client.to_string();
        storage.save(CONFIG_NAME, &config.to_string()?)?;
        info!(
            "Starting node: {} = {}",
            config.our_node.info, config.our_node.id
        );

        // Circular chicken-egg problem: the NodeArc needs a Network. But the Network
        // needs the callback that contains NodeArc...
        // This should be replaced by a `set_cb` call to network and all dependencies.
        // Or find a better way to start processing the queues if the WebRTC receives a message...
        let cb: Box<dyn FnMut()> = Box::new(|| error!("Called while not initialized"));
        let node_process = Arc::new(Mutex::new(cb));
        let network = Network::new(config.our_node.clone(), ws, web_rtc, node_process.clone());
        let network_tx = network.input_tx.clone();
        let logic = Logic::new(config.clone());
        let logic_tx = logic.input_tx.clone();
        let arc = Arc::new(Mutex::new(NodeArc { network, logic }));

        // Now that NodeArc is initialized, the process callback can be updated with the
        // real function.
        let arc_clone = arc.clone();
        *node_process.lock().unwrap() = Box::new(move || {
            let ac = arc_clone.clone();
            spawn_block(Box::pin(async move {
                match ac.try_lock() {
                    Err(_e) => trace!("ArcNode is busy"),
                    Ok(mut nm) => loop {
                        match nm.process().await {
                            Err(e) => error!("While executing Node.process: {}", e),
                            Ok(msgs) => {
                                if msgs == 0 {
                                    break;
                                }
                            }
                        }
                    },
                }
            }))
        }) as Box<dyn FnMut()>;

        Ok(Node {
            arc,
            config,
            network_tx,
            logic_tx,
        })
    }

    /// Return a copy of the current node information
    pub fn info(&self) -> Result<NodeInfo, String> {
        Ok(self.config.clone().our_node)
    }

    /// TODO: this is only for development
    pub fn clear(&mut self) -> Result<(), String> {
        self.network_tx
            .send(NInput::ClearNodes)
            .map_err(|e| e.to_string())
    }

    /// Requests a list of all connected nodes
    pub fn list(&mut self) -> Result<(), String> {
        self.network_tx
            .send(NInput::UpdateList)
            .map_err(|e| e.to_string())
    }

    /// TODO: remove ping and send - they should be called only by the Logic class.
    /// Pings all known nodes
    pub async fn ping(&mut self, msg: &str) -> Result<(), String> {
        self.logic_tx
            .send(LInput::PingAll(msg.to_string()))
            .map_err(|e| e.to_string())
    }

    /// Sends a message over webrtc to a node. The node must already be connected
    /// through websocket to the signalling server. If the connection is not set up
    /// yet, the network stack will set up a connection with the remote node.
    pub fn send(&mut self, dst: &U256, msg: String) -> Result<(), String> {
        self.network_tx
            .send(NInput::WebRTC(dst.clone(), msg))
            .map_err(|e| e.to_string())
    }

    /// Start processing of network and logic messages, in case they haven't been
    /// called automatically.
    pub async fn process(&mut self) -> Result<usize, String> {
        if let Ok(mut arc) = self.arc.try_lock() {
            return arc.process().await;
        }
        Err("Couldn't get lock for NodeArc".into())
    }

    /// Gets the current list of available nodes
    pub fn get_list(&mut self) -> Vec<NodeInfo> {
        if let Ok(arc) = self.arc.try_lock() {
            return arc.network.get_list();
        }
        vec![]
    }

    /// Returns a copy of the logic stats
    pub fn stats(&self) -> Result<HashMap<U256, Stat>, String> {
        if let Ok(arc) = self.arc.try_lock() {
            return Ok(arc.logic.stats.clone());
        }
        Err("Couldn't lock arc".to_string())
    }

    /// Static method

    /// Updates the config of the node
    pub fn set_config(storage: Box<dyn DataStorage>, config: &str) -> Result<(), String> {
        storage.save(CONFIG_NAME, config)
    }
}

/// NodeArc hodsl the network and logic structure, so that they
impl NodeArc {
    pub async fn process(&mut self) -> Result<usize, String> {
        Ok(self.process_logic()?
            + self.process_network()?
            + self.logic.process().await?
            + self.network.process().await?)
    }

    fn process_network(&mut self) -> Result<usize, String> {
        let msgs: Vec<NOutput> = self.network.output_rx.try_iter().collect();
        let size = msgs.len();
        for msg in msgs {
            match msg {
                NOutput::WebRTC(id, msg) => {
                    self.logic
                        .input_tx
                        .send(LInput::WebRTC(id, msg))
                        .map_err(|e| e.to_string())?;
                }
                NOutput::UpdateList(list) => self
                    .logic
                    .input_tx
                    .send(LInput::SetNodes(list))
                    .map_err(|e| e.to_string())?,
                NOutput::State(id, dir, c, s) => self
                    .logic
                    .input_tx
                    .send(LInput::ConnStat(id, dir, c, s))
                    .map_err(|e| e.to_string())?,
            }
        }
        Ok(size)
    }

    fn process_logic(&mut self) -> Result<usize, String> {
        let msgs: Vec<LOutput> = self.logic.output_rx.try_iter().collect();
        let size = msgs.len();
        for msg in msgs {
            match msg {
                LOutput::WebRTC(id, msg) => self
                    .network
                    .input_tx
                    .send(NInput::WebRTC(id, msg))
                    .map_err(|e| e.to_string())?,
                LOutput::SendStats(s) => self
                    .network
                    .input_tx
                    .send(NInput::SendStats(s))
                    .map_err(|e| e.to_string())?,
            }
        }
        Ok(size)
    }
}
