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

impl std::fmt::Display for RouterOut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouterOut::NodeInfoAvailable(node_infos) => {
                write!(f, "NodeInfoAvailable({})", node_infos.len())
            }
            RouterOut::NodeIDsConnected(connected) => {
                write!(f, "NodeIDsConnected({})", connected.0.len())
            }
            RouterOut::NodeInfosConnected(node_infos) => {
                write!(f, "NodeInfosConnected({})", node_infos.len())
            }
            RouterOut::NetworkWrapperFromNetwork(node_id, _) => {
                write!(f, "NetworkWrapperFromNetwork({}, ...)", node_id)
            }
            RouterOut::Disconnected(node_id) => write!(f, "Disconnected({})", node_id),
            RouterOut::Connected(node_id) => write!(f, "Connected({})", node_id),
            RouterOut::SystemConfig(config) => write!(f, "SystemConfig({:?})", config),
        }
    }
}
