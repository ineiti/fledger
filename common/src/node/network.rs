use ed25519_dalek::Signer;
use log::{info, warn};
use std::{
    collections::HashMap,
    sync::mpsc::{channel, Sender},
    sync::{mpsc::Receiver, Arc, Mutex},
};
use thiserror::Error;
use flutils::{nodeids::U256, utils::block_on};

use crate::{
    broker::{Broker, BrokerError, Subsystem, SubsystemListener},
    node::{
        config::{NodeConfig, NodeInfo},
        modules::messages::NodeMessage,
    },
    signal::{
        web_rtc::{
            ConnType, MessageAnnounce, NodeStat, SignalingState, WSSignalMessage,
            WebRTCConnectionState, WebRTCSpawner, WebSocketMessage,
        },
        websocket::{WSError, WSMessage, WebSocketConnection},
    },
};

pub mod connection_state;
pub mod node_connection;
use connection_state::CSEnum;
use node_connection::{NCError, NodeConnection};

use super::modules::messages::BrokerMessage;

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
    #[error("Cannot connect to myself")]
    ConnectMyself,
    #[error(transparent)]
    WebSocket(#[from] WSError),
    #[error(transparent)]
    SerdeJSON(#[from] serde_json::Error),
    #[error(transparent)]
    NodeConnection(#[from] NCError),
    #[error(transparent)]
    Broker(#[from] BrokerError),
}

pub struct Network {
    inner: Arc<Mutex<Inner>>,
    broker_tx: Sender<BrokerMessage>,
}

impl Network {
    pub fn start(
        broker: Broker<BrokerMessage>,
        node_config: NodeConfig,
        ws: Box<dyn WebSocketConnection>,
        web_rtc: WebRTCSpawner,
    ) {
        let (broker_tx, broker_rx) = channel::<BrokerMessage>();
        broker
            .clone()
            .add_subsystem(Subsystem::Handler(Box::new(Self {
                inner: Arc::new(Mutex::new(Inner::new(
                    broker,
                    node_config,
                    broker_rx,
                    ws,
                    web_rtc,
                ))),
                broker_tx,
            })))
            .expect("Couldn't add subsystem");
    }
}

impl SubsystemListener<BrokerMessage> for Network {
    fn messages(&mut self, bms: Vec<&BrokerMessage>) -> Vec<BrokerMessage> {
        let inner_cl = Arc::clone(&self.inner);
        for bm in bms.iter().map(|&b| b.clone()) {
            self.broker_tx.send(bm).expect("Send broker message");
        }
        block_on(async move {
            // Because block_on puts things in the background for wasm, it's not
            // really blocking, and multiple calls will arrive here at the same
            // time. Because both method calls read from a channel, it's not a problem
            // if the `try_lock` fails. It will get the messages the next time around.
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
    node_config: NodeConfig,
    broker: Broker<BrokerMessage>,
    broker_rx: Receiver<BrokerMessage>,
}

/// Inner combines a websocket to connect to the signal server with
/// a WebRTC trait to connect to other nodes.
/// It supports setting up automatic connections to other nodes.
impl Inner {
    pub fn new(
        broker: Broker<BrokerMessage>,
        node_config: NodeConfig,
        broker_rx: Receiver<BrokerMessage>,
        mut ws: Box<dyn WebSocketConnection>,
        web_rtc: WebRTCSpawner,
    ) -> Self {
        let (ws_tx, ws_rx) = channel::<WSMessage>();
        let mut broker_clone = broker.clone();
        ws.set_cb_wsmessage(Box::new(move |msg| {
            if let Err(e) = ws_tx.send(msg) {
                warn!("Couldn't send msg over ws-channel: {}", e);
            }
            if broker_clone.process().is_err() {
                warn!("Couldn't process broker");
            }
        }));
        Self {
            ws,
            ws_rx,
            broker_rx,
            web_rtc: Arc::new(Mutex::new(web_rtc)),
            connections: HashMap::new(),
            node_config,
            broker,
        }
    }

    async fn process_websocket(&mut self) -> Result<usize, NetworkError> {
        let msgs: Vec<WSMessage> = self.ws_rx.try_iter().collect();
        for msg in &msgs {
            if let WSMessage::MessageString(s) = msg {
                self.process_msg(s.parse::<WebSocketMessage>()?.msg).await?;
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
                let _ = self
                    .broker
                    .emit_msg(BrokerMessage::Network(BrokerNetwork::UpdateList(list)))?;
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
            if let BrokerMessage::Network(bn) = bm {
                match bn {
                    BrokerNetwork::NodeMessageOut(nm) => {
                        log::trace!(
                            "{}->{}: {:?}",
                            self.node_config.our_node.get_id(),
                            nm.id,
                            nm.msg
                        );
                        self.send(&nm.id, serde_json::to_string(&nm.msg)?).await?
                    }
                    BrokerNetwork::SendStats(ss) => {
                        self.ws_send(WSSignalMessage::NodeStats(ss.clone()))?
                    }
                    BrokerNetwork::UpdateListRequest => {
                        self.ws_send(WSSignalMessage::ListIDsRequest)?
                    }
                    BrokerNetwork::WebSocket(msg) => self.ws_send(msg)?,
                    BrokerNetwork::Connect(id) => self.connect(&id).await?,
                    BrokerNetwork::Disconnect(id) => self.disconnect(&id).await?,
                    _ => continue,
                }
            }
        }
        Ok(())
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

    /// Connect to the given node.
    async fn connect(&mut self, dst: &U256) -> Result<(), NetworkError> {
        self.get_connection(dst).await?;
        self.broker
            .emit_msg(BrokerMessage::Network(BrokerNetwork::Connected(*dst)))?;
        Ok(())
    }

    /// Disconnects from a given node.
    async fn disconnect(&mut self, dst: &U256) -> Result<(), NetworkError> {
        // TODO: Actually disconnect and listen for nodes that have been disconnected due to timeouts.
        self.broker
            .emit_msg(BrokerMessage::Network(BrokerNetwork::Disconnected(*dst)))?;
        Ok(())
    }

    /// Returns a connection to the given id. If the connection does not exist yet, it will
    /// start a new connection and put the message in a queue to be sent once the connection
    /// is established.
    async fn get_connection(&mut self, id: &U256) -> Result<&mut NodeConnection, NetworkError> {
        if *id == self.node_config.our_node.get_id() {
            return Err(NetworkError::ConnectMyself);
        }

        if !self.connections.contains_key(id) {
            self.connections.insert(
                *id,
                NodeConnection::new(
                    Arc::clone(&self.web_rtc),
                    self.broker.clone(),
                    self.node_config.our_node.get_id(),
                    *id,
                )
                .await?,
            );
        }
        self.connections
            .get_mut(id)
            .ok_or(NetworkError::ConnectionMissing)
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum BrokerNetwork {
    NodeMessageIn(NodeMessage),
    NodeMessageOut(NodeMessage),
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

impl From<BrokerNetwork> for BrokerMessage {
    fn from(msg: BrokerNetwork) -> Self {
        Self::Network(msg)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NetworkConnectionState {
    pub id: U256,
    pub dir: WebRTCConnectionState,
    pub c: CSEnum,
    pub s: Option<ConnStats>,
}

impl From<NetworkConnectionState> for BrokerNetwork {
    fn from(msg: NetworkConnectionState) -> Self {
        Self::ConnectionState(msg)
    }
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
