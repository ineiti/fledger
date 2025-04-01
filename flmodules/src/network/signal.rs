//! # The signalling server implementation
//!
//! This structure implements a signalling server.
//! It communicates using [`WSSignalMessageToNode`] and [`WSSignalMessageFromNode`] messages
//! with the nodes.
//!
//! # Node connection to the signalling server
//!
//! If a node wants to connect to the signalling server, it needs to prove it holds the
//! private key corresponding to its public key by signing a random message sent by
//! the signalling server:
//!
//! - Server sends [`WSSignalMessageToNode::Challenge`] with the current version of the
//! server and a 256-bit random challenge
//! - Node sends [`WSSignalMessageFromNode::Announce`] containing the [`MessageAnnounce`]
//! with the node-information and a signature of the challenge
//! - Server sends [`WSSignalMessageToNode::ListIDsReply`] if the signature has been
//! verified successfully, else it waits for another announce-message
//!
//! # WebRTC signalling setup
//!
//! Once a node has been connected to the signalling server, other nodes can start a
//! WebRTC connection with it.
//! For this, the nodes will send several [`WSSignalMessageFromNode::PeerSetup`] messages
//! that the signalling server will redirect to the other node.
//! These messages contain the [`PeerInfo`] and the [`crate::web_rtc::messages::PeerMessage`] needed to setup a WebRTC connection.
//! One of the nodes is the initializer, while the other is the follower:
//!
//! - Initializer sends a [`crate::web_rtc::messages::PeerMessage::Init`] to the follower
//! - The follower answers with a [`crate::web_rtc::messages::PeerMessage::Offer`] containing
//! information necessary for the setup of the connection
//! - The initializer uses the offer to start its connection and replies with a
//! [`crate::web_rtc::messages::PeerMessage::Answer`], also containing information for the setting
//! up of the connection
//!
//! Once the first message has been sent, both nodes will start to exchange
//! [`crate::web_rtc::messages::PeerMessage::IceCandidate`] messages which contain information
//! about possible connection candidates between the two nodes.
//! These messages will even continue once the connection has been setup, and can be used for
//! example to create a nice handover when the network connection changes.
//!
//! After the connections are set up, only the `IceCandidate` messages are exchanged between the
//! nodes.
//!
//! # Usage of the signalling server
//!
//! You can find an example of how the signalling server is used in
//! <https://github.com/ineiti/fledger/tree/0.7.0/cli/flsignal/src/main.rs>

use bimap::BiMap;
use serde::{Deserialize, Serialize};
use serde_with::{hex::Hex, serde_as};
use std::{collections::HashMap, fmt::Formatter};

use crate::{flo::realm::RealmID, nodeconfig::NodeInfo, timer::Timer};
use flarch::{
    broker::{Broker, SubsystemHandler, TranslateFrom, TranslateInto},
    nodeids::{NodeID, U256},
    platform_async_trait,
    web_rtc::{
        messages::PeerInfo,
        websocket::{BrokerWSServer, WSServerIn, WSServerOut},
    },
};

pub type BrokerSignal = Broker<SignalIn, SignalOut>;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct FledgerConfig {
    pub system_realm: Option<RealmID>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct SignalConfig {
    pub ttl_minutes: u16,
    pub system_realm: Option<RealmID>,
}

#[derive(Clone)]
/// Messages for the signalling server
pub enum SignalIn {
    /// One minute timer clock for removing stale connections
    Timer,
    /// Message coming from the WebSocket server.
    WSServer(WSServerOut),
    /// Stop the signalling server
    Stop,
}

#[derive(Clone, Debug, PartialEq)]
/// Messages sent by the signalling server to an eventual listener
pub enum SignalOut {
    /// Statistics about the connected nodes
    NodeStats(Vec<NodeStat>),
    /// Whenever a new node has joined the signalling server
    NewNode(NodeID),
    /// Messages going to the WebSocketServer
    WSServer(WSServerIn),
    /// If the server has been stopped
    Stopped,
}

/// This implements a signalling server.
/// It can be used for tests, in the cli implementation, and
/// will also be used later directly in the network struct to allow for direct node-node setups.
///
/// It handles the setup phase where the nodes authenticate themselves to the server, and passes
/// PeerInfo messages between nodes.
/// It also handles statistics by forwarding NodeStats to a listener.
pub struct SignalServer {
    connection_ids: BiMap<U256, usize>,
    info: HashMap<U256, NodeInfo>,
    ttl: HashMap<usize, u16>,
    config: SignalConfig,
}

/// Our current version - will change if the API is incompatible.
pub const SIGNAL_VERSION: u64 = 3;

impl SignalServer {
    /// Creates a new [`SignalServer`].
    /// `ttl_minutes` is the minimum time an idle node will be
    /// kept in the list.
    pub async fn new(
        ws_server: BrokerWSServer,
        config: SignalConfig,
    ) -> anyhow::Result<BrokerSignal> {
        let mut broker = Broker::new();
        broker
            .add_handler(Box::new(SignalServer {
                connection_ids: BiMap::new(),
                info: HashMap::new(),
                ttl: HashMap::new(),
                // Add 2 to the ttl_minutes to make sure that nodes are kept at least
                // 1 minute in the list.
                config,
            }))
            .await?;
        broker.link_bi(ws_server).await?;
        Timer::start()
            .await?
            .tick_minute(broker.clone(), SignalIn::Timer)
            .await?;
        Ok(broker)
    }

    fn msg_in(&mut self, msg_in: SignalIn) -> Vec<SignalOut> {
        match msg_in {
            SignalIn::Timer => {
                self.msg_in_timer();
                vec![]
            }
            SignalIn::WSServer(msg_wss) => self.msg_wss(msg_wss),
            SignalIn::Stop => vec![SignalOut::WSServer(WSServerIn::Stop)],
        }
    }

    fn msg_wss(&mut self, msg: WSServerOut) -> Vec<SignalOut> {
        match msg {
            WSServerOut::Message(index, msg_s) => {
                self.ttl
                    .entry(index.clone())
                    .and_modify(|ttl| *ttl = self.config.ttl_minutes);
                if let Ok(msg_ws) = serde_json::from_str::<WSSignalMessageFromNode>(&msg_s) {
                    return self.msg_ws_process(index, msg_ws);
                }
            }
            WSServerOut::NewConnection(index) => return self.msg_ws_connect(index),
            WSServerOut::Disconnection(id) => self.remove_node(id),
            WSServerOut::Stopped => return vec![SignalOut::Stopped],
        }
        vec![]
    }

    fn msg_in_timer(&mut self) {
        let mut to_remove = Vec::new();
        for (index, ttl) in self.ttl.iter_mut() {
            *ttl -= 1;
            if *ttl == 0 {
                log::info!("Removing idle node {index}");
                to_remove.push(*index);
            }
        }
        for id in to_remove {
            self.remove_node(id);
        }
    }

    // The id is the challange until the announcement succeeds. Then ws_announce calls
    // set_cb_message again to create a new callback using the node-id as id.
    fn msg_ws_process(&mut self, index: usize, msg: WSSignalMessageFromNode) -> Vec<SignalOut> {
        match msg {
            WSSignalMessageFromNode::Announce(ann) => self.ws_announce(index, ann),
            WSSignalMessageFromNode::ListIDsRequest => self.ws_list_ids(index),
            WSSignalMessageFromNode::PeerSetup(pi) => self.ws_peer_setup(index, pi),
            WSSignalMessageFromNode::NodeStats(ns) => self.ws_node_stats(ns),
        }
    }

    fn msg_ws_connect(&mut self, index: usize) -> Vec<SignalOut> {
        log::debug!("Sending challenge to new connection");
        let challenge = U256::rnd();
        self.connection_ids.insert(challenge, index);
        self.ttl.insert(index, self.config.ttl_minutes);
        let challenge_msg =
            serde_json::to_string(&WSSignalMessageToNode::Challenge(SIGNAL_VERSION, challenge))
                .unwrap();
        vec![SignalOut::WSServer(WSServerIn::Message(
            index,
            challenge_msg,
        ))]
    }

    fn ws_announce(&mut self, index: usize, msg: MessageAnnounce) -> Vec<SignalOut> {
        let challenge = match self.connection_ids.get_by_right(&index) {
            Some(id) => id,
            None => {
                log::warn!("Got an announcement message without challenge.");
                return vec![];
            }
        };
        if !msg.node_info.verify(&challenge.to_bytes(), &msg.signature) {
            log::warn!("Got node with wrong signature");
            return vec![];
        }
        let id = msg.node_info.get_id();
        self.connection_ids.insert(id, index);

        log::info!("Registration of node-id {}: {}", id, msg.node_info.name);
        self.info.insert(id, msg.node_info);
        let mut msgs = self.send_msg_node(
            index,
            WSSignalMessageToNode::SystemConfig(FledgerConfig {
                system_realm: self.config.system_realm.clone(),
            }),
        );
        msgs.push(SignalOut::NewNode(id));
        msgs
    }

    fn ws_list_ids(&mut self, id: usize) -> Vec<SignalOut> {
        self.send_msg_node(
            id,
            WSSignalMessageToNode::ListIDsReply(self.info.values().cloned().collect()),
        )
    }

    fn ws_peer_setup(&mut self, index: usize, pi: PeerInfo) -> Vec<SignalOut> {
        let id = match self.connection_ids.get_by_right(&index) {
            Some(id) => id,
            None => {
                log::warn!("Got a peer-setup message without challenge.");
                return vec![];
            }
        };
        log::trace!("Node {} sent peer setup: {:?}", id, pi);
        if let Some(dst) = pi.get_remote(id) {
            if let Some(dst_index) = self.connection_ids.get_by_left(&dst) {
                return self.send_msg_node(*dst_index, WSSignalMessageToNode::PeerSetup(pi));
            }
        }
        vec![]
    }

    fn ws_node_stats(&mut self, ns: Vec<NodeStat>) -> Vec<SignalOut> {
        vec![SignalOut::NodeStats(ns)]
    }

    fn send_msg_node(&self, index: usize, msg: WSSignalMessageToNode) -> Vec<SignalOut> {
        vec![SignalOut::WSServer(WSServerIn::Message(
            index,
            serde_json::to_string(&msg).unwrap(),
        ))]
    }

    fn remove_node(&mut self, index: usize) {
        log::info!("Removing node {index} from {:?}", self.info);
        if let Some((id, _)) = self.connection_ids.remove_by_right(&index) {
            self.info.remove(&id);
        }
        log::info!("Info is now: {:?}", self.info);
        self.ttl.remove(&index);
    }
}

#[platform_async_trait()]
impl SubsystemHandler<SignalIn, SignalOut> for SignalServer {
    async fn messages(&mut self, msgs: Vec<SignalIn>) -> Vec<SignalOut> {
        msgs.into_iter().flat_map(|msg| self.msg_in(msg)).collect()
    }
}

impl TranslateFrom<WSServerOut> for SignalIn {
    fn translate(msg: WSServerOut) -> Option<Self> {
        Some(SignalIn::WSServer(msg))
    }
}
impl TranslateInto<WSServerIn> for SignalOut {
    fn translate(self) -> Option<WSServerIn> {
        if let SignalOut::WSServer(msg_wss) = self {
            Some(msg_wss)
        } else {
            None
        }
    }
}

/// Message is a list of messages to be sent between the node and the signal server.
///
/// When a new node connects to the signalling server, the server starts by sending
/// a "Challenge" to the node.
/// The node can then announce itself using that challenge.
/// - Challenge is sent by the signalling server to the node
/// - ListIDsReply contains the NodeInfos of the currently connected nodes
/// - PeerRequest is sent by a node to ask to connect to another node. The
/// server will send a 'PeerReply' to the corresponding node, which will continue
/// the protocol by sending its own PeerRequest.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub enum WSSignalMessageToNode {
    /// Sends the current version of the signalling server and a random 256 bit number for authentication
    Challenge(u64, U256),
    /// A list of currently connected nodes
    ListIDsReply(Vec<NodeInfo>),
    /// Information for setting up a WebRTC connection
    PeerSetup(PeerInfo),
    /// General configuration for the system
    SystemConfig(FledgerConfig),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
/// A message coming from a node
pub enum WSSignalMessageFromNode {
    /// The reply to a [`WSSignalMessageToNode::Challenge`] message, containing information about the
    /// node, as well as a signature on the random 256 bit number being sent in the challenge.
    Announce(MessageAnnounce),
    /// Request an updated list of the currently connected nodes
    ListIDsRequest,
    /// Connection information for setting up a WebRTC connection
    PeerSetup(PeerInfo),
    /// Some statistics about its connections from the remote node
    NodeStats(Vec<NodeStat>),
}

impl std::fmt::Display for WSSignalMessageToNode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            WSSignalMessageToNode::Challenge(_, _) => write!(f, "Challenge"),
            WSSignalMessageToNode::ListIDsReply(_) => write!(f, "ListIDsReply"),
            WSSignalMessageToNode::PeerSetup(_) => write!(f, "PeerSetup"),
            WSSignalMessageToNode::SystemConfig(_) => write!(f, "SystemConfig"),
        }
    }
}

impl std::fmt::Display for WSSignalMessageFromNode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            WSSignalMessageFromNode::Announce(_) => write!(f, "Announce"),
            WSSignalMessageFromNode::ListIDsRequest => write!(f, "ListIDsRequest"),
            WSSignalMessageFromNode::PeerSetup(_) => write!(f, "PeerSetup"),
            WSSignalMessageFromNode::NodeStats(_) => write!(f, "NodeStats"),
        }
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
/// What a node sends when it connects to the signalling server
pub struct MessageAnnounce {
    /// The signalling server version the node knows
    pub version: u64,
    /// The challenge the server sent to this node.
    /// If the challenge is not correct, nothing happens, and the server waits for the
    /// correct challenge to arrive
    pub challenge: U256,
    /// Information about the remote node.
    /// The [`NodeInfo.get_id()`] will be used as the ID of this node.
    pub node_info: NodeInfo,
    #[serde_as(as = "Hex")]
    /// The signature of the challenge with the private key of the node.
    pub signature: Vec<u8>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
/// Some statistics about the connections to other nodes.
pub struct NodeStat {
    /// The id of the remote node
    pub id: NodeID,
    /// Some version
    pub version: String,
    /// The round-trip time in milliseconds
    pub ping_ms: u32,
    /// How many ping-packets have been received from this node
    pub ping_rx: u32,
}

impl std::fmt::Debug for SignalIn {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            SignalIn::WSServer(_) => write!(f, "WebSocket"),
            SignalIn::Timer => write!(f, "Timer"),
            SignalIn::Stop => write!(f, "Stop"),
        }
    }
}
