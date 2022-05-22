use ed25519_compact::{KeyPair, Noise, PublicKey, Seed, Signature};
use flmodules::nodeids::U256;
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
    /// Name of the node, up to 256 bytes
    pub name: String,
    /// What client this node runs on - "Node" or the navigator id
    pub client: String,
    /// the public key of the node
    pub pubkey: Vec<u8>,
}

impl NodeInfo {
    /// Creates a new NodeInfo with a random name.
    pub fn new(pubkey: PublicKey) -> NodeInfo {
        NodeInfo {
            name: names::Generator::default().next().unwrap(),
            client: "Node".to_string(),
            pubkey: pubkey.as_ref().to_vec(),
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
            name: nit.info,
            client: nit.client,
            pubkey: nit.pubkey.ok_or(ConfigError::PublicKeyMissing)?,
        })
    }
}

impl Debug for NodeInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        let pubkey: String = self.pubkey.iter().map(|b| format!("{:02x}", b)).collect();

        write!(
            f,
            "NodeInfo: {{ info: '{}', client: '{}', pubkey: {} }}",
            self.name, self.client, pubkey
        )
    }
}

impl PartialEq for NodeInfo {
    fn eq(&self, other: &Self) -> bool {
        self.get_id() == other.get_id()
    }
}

/// NodeConfig is stored on the node itself and contains the private key.
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeConfig {
    /// info about this node
    pub info: NodeInfo,
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
            info: NodeInfo::new(keypair.pk),
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

    pub fn sign(&self, msg: [u8; 32]) -> Vec<u8> {
        let keypair = KeyPair::from_slice(&self.keypair).unwrap();
        keypair.sk.sign(&msg, Some(Noise::default())).to_vec()
    }
}

impl Clone for NodeConfig {
    fn clone(&self) -> Self {
        NodeConfig {
            info: self.info.clone(),
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
            info: our_node,
            keypair,
        })
    }
}

/// Toml representation of the NodeConfig
#[derive(Debug, Deserialize, Serialize)]
struct NodeConfigToml {
    pub keypair: Option<Vec<u8>>,
    pub our_node: Option<NodeInfoToml>,
}

impl From<&NodeConfig> for NodeConfigToml {
    fn from(nc: &NodeConfig) -> Self {
        NodeConfigToml {
            our_node: Some((&nc.info).into()),
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
}

impl From<&NodeInfo> for NodeInfoToml {
    fn from(ni: &NodeInfo) -> Self {
        NodeInfoToml {
            id: None,
            info: ni.name.clone(),
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
    fn save_load() -> Result<(), ConfigError> {
        let nc = NodeConfig::new();
        let nc_str = nc.to_string()?;
        let nc_clone = NodeConfig::try_from(nc_str)?;
        assert_eq!(nc.keypair, nc_clone.keypair);
        assert_eq!(nc.info.pubkey, nc_clone.info.pubkey);
        Ok(())
    }
}
