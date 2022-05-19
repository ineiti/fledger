use ed25519_compact::{KeyPair, PublicKey, Seed, Noise, Signature};
use flutils::nodeids::U256;
use serde_derive::{Deserialize, Serialize};
use std::{
    convert::TryFrom,
    fmt::{Debug, Error, Formatter},
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Didn't find public key")]
    PublicKeyMissing,
    #[error(transparent)]
    DecodeToml1(#[from] toml::de::Error),
    #[error(transparent)]
    DecodeToml2(#[from] toml::ser::Error),
}

/// NodeInfo is the public information of the node.
#[derive(Deserialize, Serialize, Clone)]
pub struct NodeInfo {
    /// Free text info, limited to 256 characters
    pub info: String,
    /// What client this node runs on - "Node" or the navigator id
    pub client: String,
    /// the public key of the node
    pub pubkey: Vec<u8>,
    /// node capacities of what this node can do
    pub node_capacities: NodeCapacities,
}

impl NodeInfo {
    /// Creates a new NodeInfo with a random name.
    pub fn new(pubkey: PublicKey) -> NodeInfo {
        NodeInfo {
            info: names::Generator::default().next().unwrap(),
            client: "Node".to_string(),
            pubkey: pubkey.as_ref().to_vec(),
            node_capacities: NodeCapacities::new(),
        }
    }

    /// Returns the unique id, based on the public key.
    pub fn get_id(&self) -> U256 {
        let a: [u8; PublicKey::BYTES] = self.pubkey.clone().try_into().unwrap();
        U256::from(a)
    }

    pub fn verify(&self, msg: &[u8], sig_bytes: &[u8]) -> bool {
        let pubkey = PublicKey::from_slice(&self.pubkey).unwrap();
        let sig = match Signature::from_slice(sig_bytes) {
            Ok(sig) => sig,
            Err(_) => return false,
        };
        pubkey.verify(msg, &sig).is_ok()
    }
}

impl TryFrom<NodeInfoToml> for NodeInfo {
    type Error = ConfigError;
    fn try_from(nit: NodeInfoToml) -> Result<Self, ConfigError> {
        Ok(NodeInfo {
            info: nit.info,
            client: nit.client,
            pubkey: nit.pubkey.ok_or(ConfigError::PublicKeyMissing)?,
            node_capacities: nit.node_capacities.into(),
        })
    }
}

impl Debug for NodeInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        let pubkey: String = self.pubkey.iter().map(|b| format!("{:02x}", b)).collect();

        write!(
            f,
            "NodeInfo: {{ info: '{}', client: '{}', node_capacities: {:?}, pubkey: {} }}",
            self.info, self.client, self.node_capacities, pubkey
        )
    }
}

impl PartialEq for NodeInfo {
    fn eq(&self, other: &Self) -> bool {
        self.get_id() == other.get_id()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
/// This holds all boolean node capacities. Currently the following are implemented:
/// - leader: indicates a node that will store all relevant messages and serve them to clients
pub struct NodeCapacities {
    pub leader: bool,
}

impl NodeCapacities {
    /// Returns an initialized structure
    pub fn new() -> Self {
        Self { leader: false }
    }
}

impl From<Option<NodeCapacitiesToml>> for NodeCapacities {
    fn from(nct: Option<NodeCapacitiesToml>) -> Self {
        match nct {
            Some(n) => Self {
                leader: n.leader.unwrap_or(false),
            },
            None => NodeCapacities::new(),
        }
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
    pub keypair: Vec<u8>,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeConfig {
    /// Returns a new NodeConfig
    pub fn new() -> Self {
        let keypair = KeyPair::from_seed(Seed::default());
        NodeConfig {
            our_node: NodeInfo::new(keypair.pk),
            send_stats: 30000.,
            stats_ignore: 60000.,
            keypair: keypair.as_ref().to_vec(),
        }
    }

    /// Returns a toml representation of the config.
    pub fn to_string(&self) -> Result<String, ConfigError> {
        let str = toml::to_string(&Toml {
            v1: Some(self.into()),
        })?;
        Ok(str)
    }

    pub fn sign(&self, msg: [u8; 32]) -> Vec<u8>{
        let keypair = KeyPair::from_slice(&self.keypair).unwrap();
        keypair.sk.sign(&msg, Some(Noise::default())).to_vec()
    }
}

impl Clone for NodeConfig {
    fn clone(&self) -> Self {
        NodeConfig {
            send_stats: self.send_stats,
            stats_ignore: self.stats_ignore,
            our_node: self.our_node.clone(),
            keypair: self.keypair.clone(),
        }
    }
}

/// Parses the string as a config for the node. If the ledger is not available, it returns an error.
/// If the our_node is missing, it is created.
impl TryFrom<String> for NodeConfig {
    type Error = ConfigError;
    fn try_from(str: String) -> Result<Self, ConfigError> {
        let t: Toml = if !str.is_empty() {
            toml::from_str(str.as_str())?
        } else {
            Toml { v1: None }
        };

        if t.v1.is_none() {
            return Ok(NodeConfig::new());
        }
        let nct = t.v1.unwrap();
        let keypair = match nct.keypair {
            Some(kp) => kp,
            None => KeyPair::generate().as_ref().into(),
        };
        let kp = KeyPair::from_slice(&keypair).unwrap();
        let our_node = match nct.our_node {
            Some(mut on) => {
                on.pubkey.replace(kp.pk.as_ref().to_vec());
                NodeInfo::try_from(on)?
            }
            None => NodeInfo::new(kp.pk),
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
    pub keypair: Option<Vec<u8>>,
    pub our_node: Option<NodeInfoToml>,
}

impl From<&NodeConfig> for NodeConfigToml {
    fn from(nc: &NodeConfig) -> Self {
        NodeConfigToml {
            send_stats: Some(nc.send_stats),
            stats_ignore: Some(nc.stats_ignore),
            our_node: Some((&nc.our_node).into()),
            keypair: Some(nc.keypair.clone()),
        }
    }
}

/// Toml representation of the NodeInfo
#[derive(Debug, Deserialize, Serialize)]
struct NodeInfoToml {
    pub id: Option<U256>,
    pub info: String,
    pub client: String,
    pub pubkey: Option<Vec<u8>>,
    pub node_capacities: Option<NodeCapacitiesToml>,
}

impl From<&NodeInfo> for NodeInfoToml {
    fn from(ni: &NodeInfo) -> Self {
        NodeInfoToml {
            id: None,
            info: ni.info.clone(),
            client: ni.client.clone(),
            pubkey: Some(ni.pubkey.clone()),
            node_capacities: Some(NodeCapacitiesToml::from(&ni.node_capacities)),
        }
    }
}

/// Toml representation of capacities
#[derive(Debug, Deserialize, Serialize)]
struct NodeCapacitiesToml {
    pub leader: Option<bool>,
}

impl From<&NodeCapacities> for NodeCapacitiesToml {
    fn from(nc: &NodeCapacities) -> Self {
        Self {
            leader: Some(nc.leader),
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
    fn save_load() -> Result<(), ConfigError> {
        let nc = NodeConfig::new();
        let nc_str = nc.to_string()?;
        let nc_clone = NodeConfig::try_from(nc_str)?;
        assert_eq!(nc.keypair, nc_clone.keypair);
        assert_eq!(nc.our_node.pubkey, nc_clone.our_node.pubkey);
        Ok(())
    }
}