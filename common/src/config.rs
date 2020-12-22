
use rand::random;
use serde_derive::{Deserialize, Serialize};
use crate::types::U256;

pub struct NodeConfig {
    our_node: NodeInfo,
    ledger: Ledger,
}

// TODO: find good name, add private key
#[derive(Debug, Deserialize)]
pub struct Toml {
    our_node: Option<NodeInfo>,
    ledger: Ledger,
}

// TODO: add public key and an optional private key
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct NodeInfo {
    pub public: U256,
    pub info: String,
    pub ip: String,
    pub webrtc_address: String,
}

// impl Clone for NodeInfo {
//     fn clone(&self) -> NodeInfo {
        // NodeInfo{
            // public: self.public.clone(),
            // info: self.info.clone(),
            // ip: self.ip.clone(),
            // webrtc_address: self.webrtc_address.clone(),
        // }
//         NodeInfo::new()
//     }
// }

// impl Copy for NodeInfo {
// }

impl NodeInfo {
    pub fn new() -> NodeInfo {
        NodeInfo{
            public: random(),
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

// Parses the string as a config for the node. If the ledger is not available, it returns an error.
// If the our_node is missing, it is created.
pub fn parse_config(file: String) -> Result<NodeConfig, toml::de::Error> {
    let t: Toml = toml::from_str(file.as_str())?;

    let our_node = match t.our_node {
        Some(n) => n,
        None => NodeInfo {
            info: "New Node".to_string(),
            ip: "".to_string(),
            webrtc_address: "".to_string(),
            public: random(),
        }
    };

    Ok(NodeConfig{
        our_node,
        ledger: t.ledger,
    })
}
