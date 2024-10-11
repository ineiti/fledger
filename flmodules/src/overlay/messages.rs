use flarch::nodeids::{NodeID, NodeIDs, U256};
use serde::{Deserialize, Serialize};

use crate::nodeconfig::NodeInfo;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModuleMessage {
    pub module: String,
    pub msg: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OverlayMessage {
    Input(OverlayIn),
    Output(OverlayOut),
    Internal(OverlayInternal),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OverlayIn {
    NodeMessageToNetwork(NodeID, ModuleMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OverlayOut {
    NodeInfoAvailable(Vec<NodeInfo>),
    NodeIDsConnected(NodeIDs),
    NodeInfosConnected(Vec<NodeInfo>),
    NodeMessageFromNetwork(NodeID, ModuleMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OverlayInternal {
    Connected(U256),
    Disconnected(U256),
    Available(Vec<NodeInfo>),
}

impl From<OverlayOut> for OverlayMessage {
    fn from(value: OverlayOut) -> Self {
        OverlayMessage::Output(value)
    }
}

impl From<OverlayIn> for OverlayMessage {
    fn from(value: OverlayIn) -> Self {
        OverlayMessage::Input(value)
    }
}

impl From<OverlayInternal> for OverlayMessage {
    fn from(value: OverlayInternal) -> Self {
        OverlayMessage::Internal(value)
    }
}
