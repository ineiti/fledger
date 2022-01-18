use crate::{broker::Subsystem, node::logic::messages::NodeMessage, types::block_on};
use ed25519_dalek::Signer;
use std::sync::mpsc::{channel, Sender};

use log::{info, warn};
use std::{
    collections::HashMap,
    sync::{mpsc::Receiver, Arc, Mutex},
};
use thiserror::Error;

use self::connection_state::CSEnum;
use crate::{
    broker::{BInput, Broker, BrokerError, BrokerMessage, SubsystemListener},
    node::{
        config::{NodeConfig, NodeInfo},
        node_data::NodeData,
    },
    signal::{
        web_rtc::{
            ConnType, MessageAnnounce, NodeStat, SignalingState, WSSignalMessage,
            WebRTCConnectionState, WebRTCSpawner, WebSocketMessage,
        },
        websocket::{WSError, WSMessage, WebSocketConnection},
    },
};
use node_connection::{NCError, NodeConnection};
use types::nodeids::U256;

pub mod connection_state;
pub mod node_connection;

#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Couldn't put in output queue")]
    OutputQueue,
    #[error("Couldn't read from input queue")]
    InputQueue,
    #[error("Got alien PeerSetup")]
    AlienPeerSetup,
    #[error("Couldn't get lock")]
    Lock,
    #[error("Connection not found")]
    ConnectionMissing,
    #[error(transparent)]
    WebSocket(#[from] WSError),
    #[error(transparent)]
    SerdeJSON(#[from] serde_json::Error),
    #[error(transparent)]
    NodeConnection(#[from] NCError),
    #[error(transparent)]
    Broker(#[from] BrokerError),
}

pub struct NetworkState {
    pub list: Vec<NodeInfo>,
}

impl NetworkState {
    pub fn new() -> Self {
        Self { list: vec![] }
    }
}

pub struct Network {
    inner: Arc<Mutex<Inner>>,
    broker_tx: Sender<BrokerMessage>,
}

impl Network {
    pub fn new(
        node_data: Arc<Mutex<NodeData>>,
        ws: Box<dyn WebSocketConnection>,
        web_rtc: WebRTCSpawner,
    ) {
        let mut broker = node_data.lock().expect("Get NodeData").broker.clone();
        let (broker_tx, broker_rx) = channel::<BrokerMessage>();
        broker
            .add_subsystem(Subsystem::Handler(Box::new(Self {
                inner: Arc::new(Mutex::new(Inner::new(broker_rx, node_data, ws, web_rtc))),
                broker_tx,
            })))
            .expect("Couldn't add subsystem");
    }
}

impl SubsystemListener for Network {
    fn messages(&mut self, bms: Vec<&BrokerMessage>) -> Vec<BInput> {
        let inner_cl = Arc::clone(&self.inner);
        for bm in bms.iter().map(|&b| b.clone()) {
            self.broker_tx.send(bm).expect("Send broker message");
        }
        block_on(async move {
            // Because block_on puts things in the background for wasm, it's not
            // really blocking, and multiple calls will arrive here at the same
            // time.
            if let Ok(mut inner) = inner_cl.try_lock() {
                if let Err(e) = inner.process_broker().await {
                    log::error!("While processing broker messages: {:?}", e);
                }
                if let Err(e) = inner.process_websocket().await {
                    log::error!("While processing websockte message: {:?}", e);
                }
            }
        });

        vec![]
    }
}

struct Inner {
    ws: Box<dyn WebSocketConnection>,
    ws_rx: Receiver<WSMessage>,
    web_rtc: Arc<Mutex<WebRTCSpawner>>,
    connections: HashMap<U256, NodeConnection>,
    node_data: Arc<Mutex<NodeData>>,
    node_config: NodeConfig,
    broker: Broker,
    broker_rx: Receiver<BrokerMessage>,
}

/// Inner combines a websocket to connect to the signal server with
/// a WebRTC trait to connect to other nodes.
/// It supports setting up automatic connections to other nodes.
impl Inner {
    pub fn new(
        broker_rx: Receiver<BrokerMessage>,
        node_data: Arc<Mutex<NodeData>>,
        mut ws: Box<dyn WebSocketConnection>,
        web_rtc: WebRTCSpawner,
    ) -> Self {
        let (ws_tx, ws_rx) = channel::<WSMessage>();
        let (node_config, broker) = {
            let nsl = node_data.lock().unwrap();
            let mut broker_clone = nsl.broker.clone();
            ws.set_cb_wsmessage(Box::new(move |msg| {
                if let Err(e) = ws_tx.send(msg) {
                    warn!("Couldn't send msg over ws-channel: {}", e);
                }
                if let Err(_) = broker_clone.process() {
                    warn!("Couldn't process broker");
                }
            }));
            (nsl.node_config.clone(), nsl.broker.clone())
        };
        Self {
            ws,
            ws_rx,
            broker_rx,
            web_rtc: Arc::new(Mutex::new(web_rtc)),
            connections: HashMap::new(),
            node_config,
            node_data,
            broker,
        }
    }

    async fn process_websocket(&mut self) -> Result<usize, NetworkError> {
        let msgs: Vec<WSMessage> = self.ws_rx.try_iter().collect();
        for msg in &msgs {
            match msg {
                WSMessage::MessageString(s) => {
                    self.process_msg(WebSocketMessage::from_str(&s)?.msg)
                        .await?;
                }
                _ => {}
            }
        }
        Ok(msgs.len())
    }

    /// Processes incoming messages from the signalling server.
    /// This can be either messages requested by this node, or connection
    /// setup requests from another node.
    async fn process_msg(&mut self, msg: WSSignalMessage) -> Result<(), NetworkError> {
        match msg {
            WSSignalMessage::Challenge(version, challenge) => {
                info!("Processing Challenge message version: {}", version);
                let ma = MessageAnnounce {
                    version,
                    challenge,
                    node_info: self.node_config.our_node.clone(),
                    signature: self.node_config.keypair.sign(&challenge.to_bytes()),
                };
                self.ws.send(
                    WebSocketMessage {
                        msg: WSSignalMessage::Announce(ma),
                    }
                    .to_string(),
                )?;
                self.ws_send(WSSignalMessage::ListIDsRequest)?;
            }
            WSSignalMessage::ListIDsReply(list) => {
                self.update_list(list)?;
            }
            WSSignalMessage::PeerSetup(pi) => {
                let remote_node = match pi.get_remote(&self.node_config.our_node.get_id()) {
                    Some(id) => id,
                    None => {
                        return Err(NetworkError::AlienPeerSetup);
                    }
                };
                self.get_connection(&remote_node)
                    .await?
                    .process_ws(pi)
                    .await?;
            }
            ws => {
                info!("Got unusable message: {:?}", ws);
            }
        }
        Ok(())
    }

    async fn process_broker(&mut self) -> Result<(), NetworkError> {
        let bms: Vec<BrokerMessage> = self.broker_rx.try_iter().collect();
        for bm in bms {
            match bm {
                BrokerMessage::Network(BrokerNetwork::WebRTC(id, msg)) => {
                    self.send(&id, msg.clone()).await?
                }
                BrokerMessage::Network(BrokerNetwork::SendStats(ss)) => {
                    self.ws_send(WSSignalMessage::NodeStats(ss.clone()))?
                }
                BrokerMessage::Network(BrokerNetwork::ClearNodes) => {
                    self.ws_send(WSSignalMessage::ClearNodes)?
                }
                BrokerMessage::Network(BrokerNetwork::UpdateListRequest) => {
                    self.ws_send(WSSignalMessage::ListIDsRequest)?
                }
                BrokerMessage::Network(BrokerNetwork::WebSocket(msg)) => self.ws_send(msg)?,
                _ => continue,
            };
        }
        Ok(())
    }

    /// Stores a node list sent from the signalling server.
    fn update_list(&mut self, list: Vec<NodeInfo>) -> Result<(), NetworkError> {
        self.node_data
            .try_lock()
            .expect("locking")
            .network_state
            .list = list
            .iter()
            .filter(|entry| entry.get_id() != self.node_config.our_node.get_id())
            .cloned()
            .collect();
        let res = self
            .broker
            .emit_bm(BrokerMessage::Network(BrokerNetwork::UpdateList(list)))
            .map(|_| ())
            .map_err(|_| NetworkError::OutputQueue);
        res
    }

    fn ws_send(&mut self, msg: WSSignalMessage) -> Result<(), NetworkError> {
        self.ws.send(WebSocketMessage { msg }.to_string())?;
        Ok(())
    }

    /// Sends a message to the node dst.
    /// If no connection is active yet, a new one will be created.
    /// NodeConnection will take care of putting the message in a queue while
    /// the setup is finishing.
    async fn send(&mut self, dst: &U256, msg: String) -> Result<(), NetworkError> {
        // log::debug!("Sending {}", msg);
        self.get_connection(dst).await?.send(msg.clone()).await?;
        Ok(())
    }

    /// Returns a connection to the given id. If the connection does not exist yet, it will
    /// start a new connection and put the message in a queue to be sent once the connection
    /// is established.
    async fn get_connection(&mut self, id: &U256) -> Result<&mut NodeConnection, NetworkError> {
        if !self.connections.contains_key(id) {
            self.connections.insert(
                id.clone(),
                NodeConnection::new(
                    Arc::clone(&self.web_rtc),
                    self.broker.clone(),
                    self.node_config.our_node.get_id(),
                    id.clone(),
                )
                .await?,
            );
        }
        self.connections
            .get_mut(&id)
            .ok_or(NetworkError::ConnectionMissing)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BrokerNetwork {
    WebRTC(U256, String),
    WebSocket(WSSignalMessage),
    SendStats(Vec<NodeStat>),
    UpdateList(Vec<NodeInfo>),
    ClearNodes,
    UpdateListRequest,
    ConnectionState(NetworkConnectionState),
    Connect(U256),
    Connected(U256),
    Disconnect(U256),
    Disconnected(U256),
}

#[derive(Debug, Clone, PartialEq)]
pub struct NetworkConnectionState {
    pub id: U256,
    pub dir: WebRTCConnectionState,
    pub c: CSEnum,
    pub s: Option<ConnStats>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConnStats {
    pub type_local: ConnType,
    pub type_remote: ConnType,
    pub signaling: SignalingState,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub delay_ms: u32,
}
