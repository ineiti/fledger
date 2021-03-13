use super::types::U256;
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct NodeInfo {
    pub id: U256,
    pub info: String,
    pub client: String,
}

impl NodeInfo {
    /// Creates a new NodeInfo with a random id.
    pub fn new() -> NodeInfo {
        NodeInfo {
            id: U256::rnd(),
            info: names::Generator::default().next().unwrap().to_string(),
            client: "Node".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeConfig {
    pub our_node: NodeInfo,
}

impl NodeConfig {
    /// Parses the string as a config for the node. If the ledger is not available, it returns an error.
    /// If the our_node is missing, it is created.
    pub fn new(str: String) -> Result<NodeConfig, String> {
        let t: Toml = if str.len() > 0 {
            toml::from_str(str.as_str()).map_err(|e| e.to_string())?
        } else {
            Toml { v1: None }
        };

        Ok(t.v1.unwrap_or(NodeConfig{
            our_node: NodeInfo::new(),
        }))
    }

    pub fn to_string(&self) -> Result<String, String> {
        toml::to_string(&Toml {
            v1: Some(self.clone()),
        })
        .map_err(|e| e.to_string())
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Toml {
    v1: Option<NodeConfig>,
}
