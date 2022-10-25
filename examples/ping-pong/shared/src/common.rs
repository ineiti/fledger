use flnet::config::NodeInfo;
use serde::{Deserialize, Serialize};

use flmodules::nodeids::NodeID;

/// A PPMessage includes messages from the network, messages to be sent to
/// the network, and receiving an updated list from the signalling server.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum PPMessage {
    /// Contains the destination-id and the message to be sent. The
    /// user only has to send the 'ping', the PingPong implementation
    /// automatically sends a 'pong' reply and requests for an updated
    /// list of nodes.
    ToNetwork(NodeID, PPMessageNode),
    /// Contains source-id of the message as well as the message itself.
    /// The user does not have to reply to incoming ping messages.
    FromNetwork(NodeID, PPMessageNode),
    /// An updated list from the signalling server.
    List(Vec<NodeInfo>),
    /// Tick once per second
    Tick,
}

/// Every  contact is started with a ping and replied with a pong.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum PPMessageNode {
    Ping,
    Pong,
}

pub struct NodeList(Vec<NodeInfo>);

impl NodeList {
    pub fn new(list: Vec<NodeInfo>) -> Self {
        Self(list)
    }

    pub fn get_name(&self, id: &NodeID) -> String {
        self.0
            .iter()
            .find(|ni| &ni.get_id() == id)
            .map(|ni| ni.name.clone())
            .unwrap_or("unknown".into())
    }

    pub fn names(&self) -> String {
        format!(
            "{:?}",
            self.0
                .iter()
                .map(|n| format!("{}", n.name))
                .collect::<Vec<String>>()
        )
    }

    pub fn update(&mut self, new_list: Vec<NodeInfo>) -> bool {
        if self.0.len() != new_list.len() {
            self.0 = new_list;
            return true;
        }

        if self.0.iter().all(|n| new_list.contains(n)){
            return false;
        }

        self.0 = new_list;
        true
    }
}
