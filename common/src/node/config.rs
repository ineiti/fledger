use crate::types::U256;
use ed25519_dalek::{Keypair, SignatureError};
use rand::rngs::OsRng;

use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct NodeInfo {
    /// Currently the id is chosen randomly.
    /// TODO: insert a public key here that is used to authenticate the node
    pub id: U256,
    /// Free text info, limited to 256 characters
    pub info: String,
    /// What client this node runs on - "Node" or the navigator id
    pub client: String,
    /// The public key in bytes
    pub pubkey_bytes: Vec<u8>,
    /// node capacities of what this node can do
    pub node_capacities: NodeCapacities,
}

impl NodeInfo {
    /// Creates a new NodeInfo with a random id, and a particular public key.
    pub fn new_from_key(pubkey_bytes: Vec<u8>) -> NodeInfo {
        NodeInfo {
            id: U256::rnd(),
            info: names::Generator::default().next().unwrap().to_string(),
            client: "Node".to_string(),
            pubkey_bytes: pubkey_bytes,
        }
    }

    /// Creates a new NodeInfo with a random id and key.
    pub fn new() -> NodeInfo {
        let mut csprng = OsRng {};
        let keypair = Keypair::generate(&mut csprng);
        NodeInfo {
            id: U256::rnd(),
            info: names::Generator::default().next().unwrap().to_string(),
            client: "Node".to_string(),
            node_capacities: NodeCapacities::new(),
            pubkey_bytes: keypair.public.to_bytes().to_vec(),
        }
    }

    /// Makes sure that all default values are set.
    pub fn set_defaults(&mut self) {
        self.node_capacities.get_or_insert(NodeCapacities::new());
        self.node_capacities.as_mut().unwrap().set_defaults();
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeConfig {
    /// interval in ms between sending two statistics of connected nodes. 0 == disabled
    pub send_stats: Option<f64>,
    /// nodes that were not active for more than stats_ignore ms will not be sent
    pub stats_ignore: Option<f64>,
    /// our_node is the actual configuration of the node
    pub our_node: NodeInfo,
    /// the cryptographic keypair as a vector of bytes
    pub keypair_bytes: Vec<u8>,
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
        let mut csprng = OsRng {};
        let keypair = Keypair::generate(&mut csprng);
        let mut nc = t.v1.unwrap_or(NodeConfig {
            our_node: NodeInfo::new_from_key(keypair.public.to_bytes().to_vec()),
            send_stats: Some(30000.),
            stats_ignore: Some(60000.),
            keypair_bytes: keypair.to_bytes().to_vec(),
        });
        nc.set_defaults();
        nc.our_node.set_defaults();
        Ok(nc)
    }

    pub fn to_string(&self) -> Result<String, String> {
        toml::to_string(&Toml {
            v1: Some(self.clone()),
        })
        .map_err(|e| e.to_string())
    }

    /// Sets the defaults for the NodeConfig.
    pub fn set_defaults(&mut self){
        self.send_stats.get_or_insert(30000.);
        self.stats_ignore.get_or_insert(60000.);
    }
    
    pub fn get_keypair(&self) -> Result<Keypair, SignatureError> {
        Keypair::from_bytes(&self.keypair_bytes)
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Toml {
    v1: Option<NodeConfig>,
}
