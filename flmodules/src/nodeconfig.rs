//! Configuration structures that define a node, including old versions for migrations.
//! Also configuration for starting a new node.
//!
//! All node-configurations can be serialized with serde and offer nice hex
//! based serializations when using text-based serializations like `yaml` or `json`.

use bitflags::bitflags;
use ed25519_compact::{KeyPair, Noise, PublicKey, Seed, Signature};
use flarch::nodeids::{NodeID, U256};
use flmacro::VersionedSerde;
use serde_derive::{Deserialize, Serialize};
use serde_with::{base64::Base64, hex::Hex, serde_as};
use std::fmt::{Debug, Formatter};
use thiserror::Error;

use crate::Modules;

/// Errors to be returned when setting up a new config
#[derive(Error, Debug)]
pub enum ConfigError {
    /// If the public key cannot be found in the toml string
    #[error("Didn't find public key")]
    PublicKeyMissing,
    /// Error while decoding the toml string
    #[error("Couldn't decode")]
    NoInfo,
    /// Serde error
    #[error(transparent)]
    DecodeYaml(#[from] serde_yaml::Error),
}

/// NodeInfo is the public information of the node.
#[serde_as]
#[derive(VersionedSerde, Clone, Hash)]
#[versions = "[NodeInfoV1,NodeInfoV2]"]
pub struct NodeInfo {
    /// Name of the node, up to 256 bytes
    pub name: String,
    /// What client this node runs on - "Node" or the navigator id
    pub client: String,
    /// the public key of the node
    #[serde_as(as = "Hex")]
    pub pubkey: Vec<u8>,
    // capabilities of this node
    #[serde(default = "Modules::all")]
    pub modules: Modules,
    #[serde(skip)]
    id: Option<NodeID>,
}

impl NodeInfo {
    /// Creates a new NodeInfo with a random name.
    pub fn new(pubkey: PublicKey) -> NodeInfo {
        NodeInfo {
            name: names::Generator::default().next().unwrap(),
            client: "libc".to_string(),
            pubkey: pubkey.as_ref().to_vec(),
            modules: Modules::all(),
            id: None,
        }
    }

    pub fn new_from_id(id: NodeID) -> NodeInfo {
        let keypair = KeyPair::from_seed(Seed::default());
        Self::new_from_id_kp(id, keypair.pk)
    }

    pub fn new_from_id_kp(id: NodeID, pubkey: PublicKey) -> NodeInfo {
        NodeInfo {
            name: names::Generator::default().next().unwrap(),
            client: "libc".to_string(),
            pubkey: pubkey.as_ref().to_vec(),
            modules: Modules::all(),
            id: Some(id),
        }
    }

    /// Returns the unique id, based on the public key.
    pub fn get_id(&self) -> U256 {
        if let Some(id) = self.id {
            return id;
        }
        let a: [u8; PublicKey::BYTES] = self.pubkey.clone().try_into().unwrap();
        U256::from(a)
    }

    /// Verifies a signature with the public key of this `NodeInfo`
    pub fn verify(&self, msg: &[u8], sig_bytes: &[u8]) -> bool {
        let pubkey = PublicKey::from_slice(&self.pubkey).unwrap();
        let sig = match Signature::from_slice(sig_bytes) {
            Ok(sig) => sig,
            Err(_) => return false,
        };
        pubkey.verify(msg, &sig).is_ok()
    }

    /// Decodes a given string as yaml and returns the corresponding `NodeInfo`.
    pub fn decode(data: &str) -> Result<Self, ConfigError> {
        Ok(serde_yaml::from_str(data)?)
    }

    /// Encodes this NodeInfo as yaml string.
    pub fn encode(&self) -> String {
        serde_yaml::to_string(&self).unwrap()
    }
}

impl Debug for NodeInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
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

impl Eq for NodeInfo {}

impl From<NodeInfoV2> for NodeInfo {
    fn from(old: NodeInfoV2) -> Self {
        Self {
            name: old.name,
            client: old.client,
            pubkey: old.pubkey,
            modules: old.modules.into(),
            id: None,
        }
    }
}

#[serde_as]
#[derive(Serialize, Deserialize, Clone)]
struct NodeInfoV2 {
    /// Name of the node, up to 256 bytes
    pub name: String,
    /// What client this node runs on - "Node" or the navigator id
    pub client: String,
    /// the public key of the node
    #[serde_as(as = "Base64")]
    pub pubkey: Vec<u8>,
    // capabilities of this node
    #[serde(default = "ModulesV2::all")]
    pub modules: ModulesV2,
    #[serde(skip)]
    _id: Option<NodeID>,
}

impl Into<Modules> for ModulesV2 {
    fn into(self) -> Modules {
        Modules::from_bits(self.bits()).unwrap_or(Modules::all())
    }
}

bitflags! {
    #[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Hash)]
    pub struct ModulesV2: u32 {
        const ENABLE_STAT = 0x1;
        const ENABLE_RAND = 0x2;
        const ENABLE_GOSSIP = 0x4;
        const ENABLE_PING = 0x8;
        const ENABLE_WEBPROXY = 0x10;
        const ENABLE_WEBPROXY_REQUESTS = 0x20;
    }
}

impl From<NodeInfoV1> for NodeInfoV2 {
    fn from(_: NodeInfoV1) -> Self {
        panic!("Old configuration not supported anymore")
    }
}

// This is not used anymore, so it's empty.
#[derive(Serialize, Deserialize, Clone)]
struct NodeInfoV1 {}

/// NodeConfig is stored on the node itself and contains the private key.
#[serde_as]
#[derive(VersionedSerde, Debug, PartialEq)]
#[versions = "[NodeConfigV1]"]
pub struct NodeConfig {
    /// info about this node
    pub info: NodeInfo,
    /// the cryptographic keypair as a vector of bytes
    #[serde_as(as = "Hex")]
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

    /// Returns a new NodeConfig with an overwriting id
    pub fn new_id(id: NodeID) -> Self {
        let keypair = KeyPair::from_seed(Seed::default());
        NodeConfig {
            info: NodeInfo::new_from_id_kp(id, keypair.pk),
            keypair: keypair.as_ref().to_vec(),
        }
    }

    /// Returns a yaml representation of the config.
    pub fn encode(&self) -> String {
        serde_yaml::to_string(&self).unwrap()
    }

    /// Returns the configuration or an error. Correctly handles
    /// old toml-configurations.
    pub fn decode(data: &str) -> Self {
        serde_yaml::from_str(data)
            .ok()
            .unwrap_or_else(|| Self::new())
    }

    /// Returns the signature on the given hash with the private
    /// key stored in the config. The hash must be of length 32
    /// bytes.
    pub fn sign(&self, hash: [u8; 32]) -> Vec<u8> {
        let keypair = KeyPair::from_slice(&self.keypair).unwrap();
        keypair.sk.sign(&hash, Some(Noise::default())).to_vec()
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

#[serde_as]
#[derive(Serialize, Deserialize, Clone)]
struct NodeConfigV1 {
    /// info about this node
    info: NodeInfoV2,
    /// the cryptographic keypair as a vector of bytes
    #[serde_as(as = "Base64")]
    keypair: Vec<u8>,
}

impl From<NodeConfigV1> for NodeConfig {
    fn from(old: NodeConfigV1) -> Self {
        Self {
            info: old.info.into(),
            keypair: old.keypair,
        }
    }
}

#[cfg(test)]
mod tests {
    use flarch::start_logging_filter_level;

    use super::*;

    #[test]
    fn save_load() -> Result<(), ConfigError> {
        start_logging_filter_level(vec![], log::LevelFilter::Debug);

        let nc = NodeConfig::new();
        let nc_str = nc.encode();
        log::debug!("NodeConfig is: {nc_str}");
        let nc_clone = NodeConfig::decode(&nc_str);
        assert_eq!(nc.keypair, nc_clone.keypair);
        assert_eq!(nc.info.pubkey, nc_clone.info.pubkey);

        let i = nc.info.clone();
        let ncv1 = NodeConfigVersion::NodeConfigV1(NodeConfigV1 {
            info: NodeInfoV2 {
                name: i.name,
                client: i.client,
                pubkey: i.pubkey,
                modules: ModulesV2::all(),
                _id: i.id,
            },
            keypair: nc.keypair.clone(),
        });
        let ncv1_str = serde_yaml::to_string(&ncv1)?;
        let ncv2 = NodeConfig::decode(&ncv1_str);
        println!("{ncv1_str}");
        assert_eq!(ncv2, nc);
        Ok(())
    }

    #[test]
    fn node_info_serde() -> anyhow::Result<()>{
        let nc = NodeConfig::new();
        let ni = nc.info;
        let ni_str = ni.encode();
        let ni_clone = NodeInfo::decode(&ni_str)?;
        assert_eq!(ni, ni_clone);
        
        Ok(())
    }
}
