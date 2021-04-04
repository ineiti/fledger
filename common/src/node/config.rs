use super::types::U256;
use serde_derive::{Deserialize, Serialize};

pub const NODE_VERSION: u64 = 0x202104041345;

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
    /// interval in ms between sending two statistics of connected nodes. 0 == disabled
    pub send_stats: Option<f64>,
    /// nodes that were not active for more than stats_ignore ms will not be sent
    pub stats_ignore: Option<f64>,
    /// our_node is the actual configuration of the node
    pub our_node: NodeInfo,
}

impl NodeConfig {
    /// Parses the string as a config for the node. If the ledger is not available, it returns an error.
    /// If the our_node is missing, it is created.
    pub fn new(str: String) -> Result<NodeConfig, String> {
        let t: Toml = if str.len() > 0 {
            toml::from_str(str.as_str()).map_err(|e| e.to_string())?
        } else {
            // Toml { v1: None }
            Toml { v1: None }
        };

        let mut nc = t.v1.unwrap_or(NodeConfig{
            our_node: NodeInfo::new(),
            send_stats: Some(30000.),
            stats_ignore: Some(60000.),
        });
        nc.send_stats.replace(nc.send_stats.unwrap_or(30000.));
        nc.stats_ignore.replace(nc.stats_ignore.unwrap_or(60000.));
        Ok(nc)
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
