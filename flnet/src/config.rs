//! Configuration structures that define a node, including old versions for migrations.
//! Also configuration for starting a new node.
//!
//! All node-configurations can be serialized with serde and offer nice hex
//! based serializations when using text-based serializations like `yaml` or `json`.

use ed25519_compact::{KeyPair, Noise, PublicKey, Seed, Signature};
use flmodules::{nodeids::U256, Modules};
use serde_derive::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};
use std::{
    convert::TryFrom,
    fmt::{Debug, Error, Formatter},
};
use thiserror::Error;

/// A configuration to set up a WebRTC connection.
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// The signalling server, defaults to "ws://localhost:8765"
    signal_server: Option<String>,
    /// The STUN server, defaults to "stun:stun.l.google.com:19302"
    stun_server: Option<HostLogin>,
    /// The TURN server, by default none
    turn_server: Option<HostLogin>,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            signal_server: Some("ws://localhost:8765".into()),
            stun_server: Some(HostLogin {
                url: "stun:stun.l.google.com:19302".into(),
                login: None,
            }),
            turn_server: None,
        }
    }
}

impl ConnectionConfig {
    pub fn new(
        signal_server: Option<String>,
        stun_server: Option<HostLogin>,
        turn_server: Option<HostLogin>,
    ) -> Self {
        Self {
            signal_server,
            stun_server,
            turn_server,
        }
    }

    /// Returns a ConnectionConfig with only the url of the signalling server set.
    pub fn from_signal(url: &str) -> Self {
        Self {
            signal_server: Some(url.into()),
            stun_server: None,
            turn_server: None,
        }
    }

    /// Returns the signalling server, or the default if none set.
    pub fn signal(&self) -> String {
        self.signal_server
            .as_ref()
            .unwrap_or(&"ws://localhost:8765".into())
            .clone()
    }

    /// Returns the STUN server, or the default if none set.
    pub fn stun(&self) -> HostLogin {
        self.stun_server
            .as_ref()
            .unwrap_or(&HostLogin::from_url("stun:stun.l.google.com:19302"))
            .clone()
    }

    /// Returns an Option to the turn server, as there is no default available publicly.
    pub fn turn(&self) -> Option<HostLogin> {
        self.turn_server.clone()
    }
}

/// A URL with an optional username/password.
#[derive(Debug, Clone)]
pub struct HostLogin {
    /// The URL for the host
    pub url: String,
    /// An optional username/password to authenticate to the host
    pub login: Option<Login>,
}

impl HostLogin {
    pub fn from_url(url: &str) -> Self {
        Self {
            url: url.into(),
            login: None,
        }
    }
}

/// A login with a username/password.
#[derive(Debug, Clone)]
pub struct Login {
    /// The username
    pub user: String,
    /// The password
    pub pass: String,
}

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
    DecodeToml1(#[from] toml::de::Error),
    /// Serde error
    #[error(transparent)]
    DecodeToml2(#[from] toml::ser::Error),
}

/// NodeInfo is the public information of the node.
#[serde_as]
#[derive(Deserialize, Serialize, Clone)]
pub struct NodeInfo {
    /// Name of the node, up to 256 bytes
    pub name: String,
    /// What client this node runs on - "Node" or the navigator id
    pub client: String,
    /// the public key of the node
    #[serde_as(as = "Base64")]
    pub pubkey: Vec<u8>,
    // capabilities of this node
    pub modules: Modules,
}

#[derive(Deserialize, Serialize, Clone)]
enum NodeInfoSave {
    NodeInfoV2(NodeInfo),
}

impl NodeInfoSave {
    fn to_latest(self) -> NodeInfo {
        match self {
            NodeInfoSave::NodeInfoV2(ni) => ni,
        }
    }
}

impl NodeInfo {
    /// Creates a new NodeInfo with a random name.
    pub fn new(pubkey: PublicKey) -> NodeInfo {
        NodeInfo {
            name: names::Generator::default().next().unwrap(),
            client: "libc".to_string(),
            pubkey: pubkey.as_ref().to_vec(),
            modules: Modules::empty(),
        }
    }

    /// Returns the unique id, based on the public key.
    pub fn get_id(&self) -> U256 {
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
        if let Ok(info) = serde_yaml::from_str::<NodeInfoSave>(data) {
            return Ok(info.to_latest());
        }
        NodeInfoV1::from_str(data)
            .ok_or(ConfigError::NoInfo)
            .map(|i| i.into())
    }

    /// Encodes this NodeInfo as yaml string.
    pub fn encode(&self) -> String {
        serde_yaml::to_string(&NodeInfoSave::NodeInfoV2(self.clone())).unwrap()
    }
}

/// NodeInfo is the public information of the node.
#[derive(Deserialize, Serialize, Clone)]
pub struct NodeInfoV1 {
    /// Name of the node, up to 256 bytes
    pub info: String,
    /// What client this node runs on - "libc" or the navigator id
    pub client: String,
    /// the public key of the node
    pub pubkey: Vec<u8>,
}

impl NodeInfoV1 {
    fn from_str(data: &str) -> Option<Self> {
        if let Ok(info) = serde_json::from_str::<NodeInfoV1>(data) {
            return Some(info);
        }
        if let Ok(info) = toml::from_str::<NodeInfoV1>(data) {
            return Some(info);
        }
        None
    }
}

impl From<NodeInfoV1> for NodeInfo {
    fn from(ni: NodeInfoV1) -> Self {
        NodeInfo {
            name: ni.info,
            client: ni.client,
            pubkey: ni.pubkey,
            modules: Modules::empty(),
        }
    }
}

impl TryFrom<NodeInfoToml> for NodeInfo {
    type Error = ConfigError;
    fn try_from(nit: NodeInfoToml) -> Result<Self, ConfigError> {
        Ok(NodeInfo {
            name: nit.info,
            client: nit.client,
            pubkey: nit.pubkey.ok_or(ConfigError::PublicKeyMissing)?,
            modules: Modules::empty(),
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
#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeConfig {
    /// info about this node
    pub info: NodeInfo,
    /// the cryptographic keypair as a vector of bytes
    #[serde_as(as = "Base64")]
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

    /// Returns a yaml representation of the config.
    pub fn encode(&self) -> String {
        serde_yaml::to_string(&NodeConfigSave::NodeConfigV1(self.clone())).unwrap()
    }

    /// Returns the configuration or an error. Correctly handles
    /// old toml-configurations.
    pub fn decode(data: &str) -> Result<Self, ConfigError> {
        if let Ok(nc) = serde_yaml::from_str::<NodeConfigSave>(data) {
            return Ok(nc.to_latest());
        }
        Self::from_toml(data)
    }

    /// Returns the signature on the given hash with the private
    /// key stored in the config. The hash must be of length 32
    /// bytes.
    pub fn sign(&self, hash: [u8; 32]) -> Vec<u8> {
        let keypair = KeyPair::from_slice(&self.keypair).unwrap();
        keypair.sk.sign(&hash, Some(Noise::default())).to_vec()
    }

    /// This is for compatibility with old nodes.
    fn from_toml(data: &str) -> Result<Self, ConfigError> {
        let t: Toml = if !data.is_empty() {
            toml::from_str(data)?
        } else {
            return Ok(NodeConfig::new());
        };

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

impl Clone for NodeConfig {
    fn clone(&self) -> Self {
        NodeConfig {
            info: self.info.clone(),
            keypair: self.keypair.clone(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
enum NodeConfigSave {
    NodeConfigV1(NodeConfig),
}

impl NodeConfigSave {
    fn to_latest(self) -> NodeConfig {
        match self {
            NodeConfigSave::NodeConfigV1(nc) => nc,
        }
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
    use flarch::start_logging;

    use super::*;

    #[test]
    fn save_load() -> Result<(), ConfigError> {
        start_logging();

        let nc = NodeConfig::new();
        let nc_str = nc.encode();
        log::debug!("NodeConfig is: {nc_str}");
        let nc_clone = NodeConfig::decode(&nc_str)?;
        assert_eq!(nc.keypair, nc_clone.keypair);
        assert_eq!(nc.info.pubkey, nc_clone.info.pubkey);
        Ok(())
    }
}
