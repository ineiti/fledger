use flarch::nodeids::{NodeID, NodeIDs};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{network::signal::FledgerConfig, nodeconfig::NodeInfo};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkWrapper {
    pub module: String,
    pub msg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RouterIn {
    NetworkWrapperToNetwork(NodeID, NetworkWrapper),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RouterOut {
    NodeInfoAvailable(Vec<NodeInfo>),
    NodeIDsConnected(NodeIDs),
    NodeInfosConnected(Vec<NodeInfo>),
    NetworkWrapperFromNetwork(NodeID, NetworkWrapper),
    Disconnected(NodeID),
    Connected(NodeID),
    SystemConfig(FledgerConfig),
}

impl NetworkWrapper {
    pub fn wrap_yaml<T: Serialize>(module: &str, msg: &T) -> anyhow::Result<Self> {
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
