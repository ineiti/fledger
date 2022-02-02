use core::f64;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use thiserror::Error;

use flutils::{
    broker::{Broker, BrokerError, Subsystem, SubsystemListener},
    nodeids::U256,
    utils::now,
};

use crate::{
    node::{
        config::{NodeConfig, NodeInfo},
        modules::messages::{BrokerMessage, Message, MessageV1, NodeMessage},
        network::{connection_state::CSEnum, BrokerNetwork, ConnStats, NetworkConnectionState},
        node_data::NodeData,
        timer::BrokerTimer,
        version::VERSION_STRING,
    },
    signal::web_rtc::{ConnType, NodeStat, WebRTCConnectionState},
};

#[derive(Debug, Error)]
pub enum SNError {
    #[error("While sending through Output Queue")]
    OutputQueue,
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Broker(#[from] BrokerError),
}

pub struct NDStats {
    pub nodes: HashMap<U256, StatNode>,
    pub list: Vec<NodeInfo>,
}

impl NDStats {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            list: vec![],
        }
    }

    pub fn update_nodes(&mut self, nodes: HashMap<U256, StatNode>) {
        self.nodes = nodes;
    }
}

impl Default for NDStats {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct StatNode {
    pub node_info: Option<NodeInfo>,
    pub ping_rx: u32,
    pub ping_tx: u32,
    pub last_contact: f64,
    pub incoming: ConnState,
    pub outgoing: ConnState,
    pub client_info: String,
}

impl StatNode {
    pub fn new(node_info: Option<NodeInfo>, last_contact: f64) -> Self {
        Self {
            node_info,
            ping_rx: 0,
            ping_tx: 0,
            last_contact,
            incoming: ConnState::Idle,
            outgoing: ConnState::Idle,
            client_info: "N/A".to_string(),
        }
    }
}

pub struct Stats {
    node_data: Arc<Mutex<NodeData>>,
    node_config: NodeConfig,
    broker: Broker<BrokerMessage>,
    last_stats: f64,
    ping_all_counter: u32,
}

impl Stats {
    pub fn start(node_data: Arc<Mutex<NodeData>>) {
        let (node_config, broker) = {
            let nd = node_data.lock().unwrap();
            (nd.node_config.clone(), nd.broker.clone())
        };
        broker
            .clone()
            .add_subsystem(Subsystem::Handler(Box::new(Stats {
                node_data,
                node_config,
                broker,
                last_stats: 0.,
                ping_all_counter: 0,
            })))
            .unwrap();
    }

    /// Check if the stats need to be sent to the signalling server.
    fn send_stats(&mut self) -> Result<(), SNError> {
        // Send statistics to the signalling server
        if self.tick(self.node_config.send_stats) {
            self.expire(self.node_config.stats_ignore);

            let stats: Vec<NodeStat> = self.collect();

            self.broker
                .enqueue_msg(BrokerMessage::Network(BrokerNetwork::SendStats(stats)));
        }
        if self.ping_all_counter == 0 {
            self.ping_all()?;
            self.ping_all_counter = 5;
        }
        self.ping_all_counter -= 1;
        Ok(())
    }

    /// Check if an update of the stats must be sent to the signalling server.
    fn tick(&mut self, send_stats: f64) -> bool {
        let now_here = now();
        if now_here > self.last_stats + send_stats {
            self.last_stats = now_here;
            return true;
        }
        false
    }

    /// Remove nodes that haven't been seen in a while.
    fn expire(&mut self, stats_ignore: f64) {
        let mut nodes = self.get_stats_nodes();
        let expired: Vec<U256> = nodes
            .iter()
            .filter(|(_k, v)| self.last_stats - v.last_contact > stats_ignore)
            .map(|(k, _v)| *k)
            .collect();
        for k in expired {
            nodes.remove(&k);
        }
        self.set_stats_nodes(nodes);
    }

    /// Collects all stats from all known nodes.
    fn collect(&self) -> Vec<NodeStat> {
        let our_id = self.node_config.our_node.get_id();
        self.get_stats_nodes()
            .iter()
            // Ignore our node and nodes that are inactive
            .filter(|(&k, _v)| k != our_id)
            .map(|(k, v)| NodeStat {
                id: *k,
                version: VERSION_STRING.to_string(),
                ping_ms: 0u32,
                ping_rx: v.ping_rx,
            })
            .collect()
    }

    /// Update or insert state information about a node
    fn upsert(&mut self, ncs: &NetworkConnectionState) -> Result<(), SNError> {
        let mut nodes = self.get_stats_nodes();
        nodes
            .entry(ncs.id)
            .or_insert_with(|| StatNode::new(None, now()));
        nodes.entry(ncs.id).and_modify(|s| {
            let cs = ConnState::from_states(ncs.c.clone(), ncs.s.clone());
            if ncs.dir == WebRTCConnectionState::Initializer {
                s.outgoing = cs;
            } else {
                s.incoming = cs;
            }
        });
        self.set_stats_nodes(nodes);
        Ok(())
    }

    /// Update a NodeInfo
    fn update_list(&mut self, nul: &[NodeInfo]) -> Result<(), SNError> {
        let mut nodes = self.get_stats_nodes();
        for ni in nul {
            let s = nodes
                .entry(ni.get_id())
                .or_insert_with(|| StatNode::new(None, now()));
            s.node_info = Some(ni.clone());
        }
        self.set_stats_nodes(nodes);
        Ok(())
    }

    /// Send a ping to all nodes.
    fn ping_all(&mut self) -> Result<(), SNError> {
        let our_id = self.node_config.our_node.get_id();
        let mut nodes = self.get_stats_nodes();
        for stat in nodes.iter_mut() {
            if let Some(ni) = stat.1.node_info.as_ref() {
                if our_id != ni.get_id() {
                    self.broker.enqueue_msg(
                        NodeMessage {
                            id: ni.get_id(),
                            msg: Message::V1(MessageV1::Ping()),
                        }
                        .output(),
                    );
                    stat.1.ping_tx += 1;
                }
            }
        }
        self.set_stats_nodes(nodes);
        Ok(())
    }

    /// Receive a ping from a node
    fn ping_rcv(&mut self, nm: &NodeMessage) -> Result<(), SNError> {
        if !matches!(nm.msg, Message::V1(MessageV1::Ping())) {
            return Ok(());
        }

        let from = nm.id;
        let mut nodes = self.get_stats_nodes();
        nodes
            .entry(from)
            .or_insert_with(|| StatNode::new(None, now()));
        nodes.entry(from).and_modify(|s| {
            s.last_contact = now();
            s.ping_rx += 1;
        });
        self.set_stats_nodes(nodes);
        Ok(())
    }

    fn get_stats_nodes(&self) -> HashMap<U256, StatNode> {
        let sn = self.node_data.lock().unwrap().stats.nodes.clone();
        sn
    }

    fn set_stats_nodes(&self, nodes: HashMap<U256, StatNode>) {
        self.node_data.lock().unwrap().stats.update_nodes(nodes);
    }
}

impl SubsystemListener<BrokerMessage> for Stats {
    fn messages(&mut self, msgs: Vec<&BrokerMessage>) -> Vec<BrokerMessage> {
        for msg in msgs {
            if let Err(e) = match msg {
                BrokerMessage::Network(BrokerNetwork::ConnectionState(cs)) => self.upsert(cs),
                BrokerMessage::Network(BrokerNetwork::UpdateList(ul)) => self.update_list(ul),
                BrokerMessage::Network(BrokerNetwork::NodeMessageIn(nm)) => self.ping_rcv(nm),
                BrokerMessage::Timer(BrokerTimer::Second) => self.send_stats(),
                _ => Ok(()),
            } {
                log::warn!("Got error {:?} while processing message {:?}", e, msg);
            }
        }
        vec![]
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum ConnState {
    Idle,
    Setup,
    Connected,
    TURN,
    STUN,
    Host,
}

impl ConnState {
    pub fn from_states(st: CSEnum, state: Option<ConnStats>) -> Self {
        match st {
            CSEnum::Idle => ConnState::Idle,
            CSEnum::Setup => ConnState::Setup,
            CSEnum::HasDataChannel => {
                if let Some(state_value) = state {
                    match state_value.type_local {
                        ConnType::Unknown => ConnState::Connected,
                        ConnType::Host => ConnState::Host,
                        ConnType::STUNPeer => ConnState::STUN,
                        ConnType::STUNServer => ConnState::STUN,
                        ConnType::TURN => ConnState::TURN,
                    }
                } else {
                    ConnState::Connected
                }
            }
        }
    }
}
