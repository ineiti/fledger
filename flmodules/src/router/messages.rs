use flarch::nodeids::{NodeID, NodeIDs, U256};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::nodeconfig::NodeInfo;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkWrapper {
    pub module: String,
    pub msg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RouterIn {
    NetworkWrapperToNetwork(NodeID, NetworkWrapper),
    Internal(RouterInternal),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RouterOut {
    NodeInfoAvailable(Vec<NodeInfo>),
    NodeIDsConnected(NodeIDs),
    NodeInfosConnected(Vec<NodeInfo>),
    NetworkWrapperFromNetwork(NodeID, NetworkWrapper),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RouterInternal {
    Connected(U256),
    Disconnected(U256),
    Available(Vec<NodeInfo>),
    MessageFromNode(U256, String),
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

impl From<RouterInternal> for RouterIn{
    fn from(value: RouterInternal) -> Self {
        RouterIn::Internal(value)
    }
}
