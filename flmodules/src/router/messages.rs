use flarch::nodeids::{NodeID, NodeIDs, U256};
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
    // TODO: Replace this with Network(NetworkOut), and add a Network(NetworkIn) to RouterOut
    Internal(RouterInternal),
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RouterInternal {
    Connected(U256),
    Disconnected(U256),
    Available(Vec<NodeInfo>),
    MessageFromNode(U256, String),
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

impl From<RouterInternal> for RouterIn {
    fn from(value: RouterInternal) -> Self {
        RouterIn::Internal(value)
    }
}
