use crate::types::U256;
use ed25519_dalek::{Keypair, PublicKey};
use rand::rngs::OsRng;
use std::convert::TryFrom;

use serde_derive::{Deserialize, Serialize};

/// NodeInfo is the public information of the node.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct NodeInfo {
    /// Free text info, limited to 256 characters
    pub info: String,
    /// What client this node runs on - "Node" or the navigator id
    pub client: String,
    /// the public key of the node
    pub pubkey: PublicKey,
    /// node capacities of what this node can do
    pub node_capacities: NodeCapacities,
}

impl NodeInfo {
    /// Creates a new NodeInfo with a random name.
    pub fn new(pubkey: PublicKey) -> NodeInfo {
        NodeInfo {
            info: names::Generator::default().next().unwrap().to_string(),
            client: "Node".to_string(),
            pubkey,
            node_capacities: NodeCapacities::new(),
        }
    }

    /// Returns the unique id, based on the public key.
    pub fn get_id(&self) -> U256 {
        self.pubkey.to_bytes().into()
    }
}

impl TryFrom<NodeInfoToml> for NodeInfo {
    type Error = String;
    fn try_from(nit: NodeInfoToml) -> Result<Self, String> {
        Ok(NodeInfo {
            info: nit.info,
            client: nit.client,
            pubkey: nit.pubkey.ok_or("No public key".to_string())?.clone(),
            node_capacities: NodeCapacities::new(),
        })
    }
}

impl PartialEq for NodeInfo {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
/// This holds all boolean node capacities. Currently the following are implemented:
/// - leader: indicates a node that will store all relevant messages and serve them to clients
pub struct NodeCapacities {
    pub leader: Option<bool>,
}

impl NodeCapacities{
    /// Returns an initialized structure
    pub fn new() -> Self{
        Self{
            leader: None,
        }.set_defaults()
    }

    /// Sets all None fields to a pre-initialized value.
    pub fn set_defaults(&mut self) -> Self{
        self.leader.get_or_insert(false);
        self.clone()
    }
}

/// NodeConfig is stored on the node itself and contains the private key.
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeConfig {
    /// interval in ms between sending two statistics of connected nodes. 0 == disabled
    pub send_stats: f64,
    /// nodes that were not active for more than stats_ignore ms will not be sent
    pub stats_ignore: f64,
    /// our_node is the actual configuration of the node
    pub our_node: NodeInfo,
    /// the cryptographic keypair as a vector of bytes
    pub keypair: Keypair,
}

impl NodeConfig {
    /// Returns a new NodeConfig
    pub fn new() -> Self {
        let mut csprng = OsRng {};
        let keypair = Keypair::generate(&mut csprng);
        NodeConfig {
            our_node: NodeInfo::new(keypair.public),
            send_stats: 30000.,
            stats_ignore: 60000.,
            keypair,
        }
    }

    /// Returns a toml representation of the config.
    pub fn to_string(&self) -> Result<String, String> {
        toml::to_string(&Toml {
            v1: Some(self.into()),
        })
        .map_err(|e| e.to_string())
    }
}

impl Clone for NodeConfig {
    fn clone(&self) -> Self {
        let keypair = Keypair::from_bytes(&self.keypair.to_bytes()).unwrap();
        NodeConfig {
            send_stats: self.send_stats,
            stats_ignore: self.stats_ignore,
            our_node: self.our_node.clone(),
            keypair,
        }
    }
}

/// Parses the string as a config for the node. If the ledger is not available, it returns an error.
/// If the our_node is missing, it is created.
impl TryFrom<String> for NodeConfig {
    type Error = String;
    fn try_from(str: String) -> Result<Self, String> {
        let t: Toml = if str.len() > 0 {
            toml::from_str(str.as_str()).map_err(|e| e.to_string())?
        } else {
            Toml { v1: None }
        };

        if t.v1.is_none() {
            return Ok(NodeConfig::new());
        }
        let nct = t.v1.unwrap();
        let keypair = match nct.keypair {
            Some(kp) => kp,
            None => {
                let mut csprng = OsRng {};
                Keypair::generate(&mut csprng)
            }
        };
        let our_node = match nct.our_node {
            Some(mut on) => {
                on.pubkey.replace(keypair.public.clone());
                NodeInfo::try_from(on)?
            }
            None => NodeInfo::new(keypair.public.clone()),
        };
        Ok(NodeConfig {
            our_node,
            send_stats: nct.send_stats.unwrap_or(30000.),
            stats_ignore: nct.stats_ignore.unwrap_or(60000.),
            keypair,
        })
    }
}

/// Toml representation of the NodeConfig
#[derive(Debug, Deserialize, Serialize)]
struct NodeConfigToml {
    pub send_stats: Option<f64>,
    pub stats_ignore: Option<f64>,
    pub keypair: Option<Keypair>,
    pub our_node: Option<NodeInfoToml>,
}

impl From<&NodeConfig> for NodeConfigToml {
    fn from(nc: &NodeConfig) -> Self {
        NodeConfigToml {
            send_stats: Some(nc.send_stats),
            stats_ignore: Some(nc.stats_ignore),
            our_node: Some((&nc.our_node).into()),
            keypair: Some(Keypair::from_bytes(&nc.keypair.to_bytes()).unwrap()),
        }
    }
}

/// Toml representation of the NodeInfo
#[derive(Debug, Deserialize, Serialize)]
struct NodeInfoToml {
    pub id: Option<U256>,
    pub info: String,
    pub client: String,
    pub pubkey: Option<PublicKey>,
}

impl From<&NodeInfo> for NodeInfoToml {
    fn from(ni: &NodeInfo) -> Self {
        NodeInfoToml {
            id: None,
            info: ni.info.clone(),
            client: ni.client.clone(),
            pubkey: Some(ni.pubkey.clone()),
        }
    }
}

/// What is stored on the node. If the configuration is too different,
/// a new version has to be added.
#[derive(Debug, Deserialize, Serialize)]
struct Toml {
    v1: Option<NodeConfigToml>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load() -> Result<(), String>{
        let nc = NodeConfig::new();
        let nc_str = nc.to_string()?;
        let nc_clone = NodeConfig::try_from(nc_str)?;
        assert_eq!(nc.keypair.to_bytes(), nc_clone.keypair.to_bytes());
        assert_eq!(nc.our_node.pubkey.to_bytes(), nc_clone.our_node.pubkey.to_bytes());
        Ok(())
    }
}
