use flarch::broker::Broker;
use serde::{Deserialize, Serialize};

use crate::node::{
    broadcast::{BroadcastFromTabs, BroadcastToTabs},
    node::{NodeIn, NodeOut},
};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ProxyIn {
    Broadcast(BroadcastFromTabs),
    Node(NodeIn),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ProxyOut {
    Broadcast(BroadcastToTabs),
    Node(NodeOut),
}

pub type BrokerProxy = Broker<ProxyIn, ProxyOut>;

pub struct Leader {}

impl Leader {
    pub fn start() -> anyhow::Result<BrokerProxy> {
        todo!()
    }
}

pub struct Follower {}

impl Follower {
    pub fn start() -> anyhow::Result<BrokerProxy> {
        todo!()
    }
}
