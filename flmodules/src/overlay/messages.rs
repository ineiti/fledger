use flarch::nodeids::{NodeID, NodeIDs, U256};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::nodeconfig::NodeInfo;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkWrapper {
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
    NetworkWrapperToNetwork(NodeID, NetworkWrapper),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OverlayOut {
    NodeInfoAvailable(Vec<NodeInfo>),
    NodeIDsConnected(NodeIDs),
    NodeInfosConnected(Vec<NodeInfo>),
    NetworkWrapperFromNetwork(NodeID, NetworkWrapper),
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

impl NetworkWrapper {
    pub fn wrap_yaml<T: Serialize>(module: &str, msg: &T) -> Result<Self, serde_yaml::Error> {
        Ok(Self {
            module: module.into(),
            msg: serde_yaml::to_string(msg)?,
        })
    }

    pub fn unwrap_yaml<T: DeserializeOwned>(&self, module: &str) -> Option<T> {
        if self.module == module {
            if let Ok(msg) = serde_yaml::from_str(&self.msg) {
                return Some(msg);
            }
        }
        None
    }
}
