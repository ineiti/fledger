use crate::types::U256;
use serde_derive::{Deserialize, Serialize};

// TODO: add public key and an optional private key
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct NodeInfo {
    pub public: U256,
    pub info: String,
    pub ip: String,
    pub webrtc_address: String,
}

impl NodeInfo {
    /// Creates a new NodeInfo with a random public key.
    /// TODO: actually implement something useful.
    pub fn new() -> NodeInfo {
        NodeInfo {
            public: U256::rnd(),
            info: "new node".to_string(),
            ip: "127".to_string(),
            webrtc_address: "something".to_string(),
        }
    }
}

// TODO: handle discovering the ledger and remove the root NodeInfo
#[derive(Debug, Deserialize)]
pub struct Ledger {
    name: String,
    root: NodeInfo,
}

#[derive(Debug)]
pub struct NodeConfig {
    pub our_node: NodeInfo,
    // pub ledger: Ledger,
}

impl NodeConfig {
    /// Parses the string as a config for the node. If the ledger is not available, it returns an error.
    /// If the our_node is missing, it is created.
    pub fn new(str: String) -> Result<NodeConfig, String> {
        let t: Toml = if str.len() > 0 {
            toml::from_str(str.as_str()).map_err(|e| e.to_string())?
        } else {
            Toml { our_node: None }
        };

        Ok(NodeConfig {
            our_node: t.our_node.unwrap_or(NodeInfo::new()),
            // ledger: t.ledger,
        })
    }

    pub fn to_string(&self) -> Result<String, String> {
        toml::to_string(&Toml {
            our_node: Some(self.our_node.clone()),
        })
        .map_err(|e| e.to_string())
    }
}

// TODO: find good name, add private key
#[derive(Debug, Deserialize, Serialize)]
struct Toml {
    our_node: Option<NodeInfo>,
    // ledger: Ledger,
}
