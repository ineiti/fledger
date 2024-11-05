use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use flarch::nodeids::{NodeID, NodeIDs};
use sha2::digest::crypto_common::rand_core::le;
use x25519_dalek::{PublicKey, StaticSecret};
use tokio::sync::RwLock;
use std::sync::Arc;
use std::collections::HashSet;
use crate::loopix::sphinx::Sphinx;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LoopixStorageSave {
    V1(LoopixStorage),
}

impl LoopixStorageSave {
    pub fn from_str(data: &str) -> Result<LoopixStorage, serde_yaml::Error> {
        Ok(serde_yaml::from_str::<LoopixStorageSave>(data)?.to_latest())
    }

    fn to_latest(self) -> LoopixStorage {
        match self {
            LoopixStorageSave::V1(es) => es,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct NetworkStorage {
    node_id: NodeID,
    #[serde(serialize_with = "serialize_static_secret", deserialize_with = "deserialize_static_secret")]
    private_key: StaticSecret,
    #[serde(serialize_with = "serialize_public_key", deserialize_with = "deserialize_public_key")]
    public_key: PublicKey,
    mixes: Vec<Vec<NodeID>>,
    providers: Vec<NodeID>,
    #[serde(serialize_with = "serialize_node_public_keys", deserialize_with = "deserialize_node_public_keys")]
    node_public_keys: HashMap<NodeID, PublicKey>,
}

impl NetworkStorage {
    pub fn get_mixes(&self) -> &Vec<Vec<NodeID>> {
        &self.mixes
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ClientStorage {
    our_provider: Option<NodeID>,
    client_to_provider_map: HashMap<NodeID, NodeID>,
}

impl ClientStorage {
    pub fn new(our_provider: Option<NodeID>, client_to_provider_map: HashMap<NodeID, NodeID>) -> Self {
        ClientStorage {
            our_provider,
            client_to_provider_map,
        }
    }

    /// given path_length 3: our node either 0, 1, or 2
    pub fn default_with_path_length(our_node_id: u32, path_length: usize) -> Self {
        if our_node_id >= path_length as u32 {
            panic!("Our node id must be less than the path length");
        }
        let our_provider = Some(NodeID::from(our_node_id + path_length as u32));

        let mut client_to_provider_map = HashMap::new();
        for i in 0..path_length {
            client_to_provider_map.insert(NodeID::from(i as u32), NodeID::from((i + path_length) as u32));
        }
        ClientStorage {
            our_provider,
            client_to_provider_map,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ProviderStorage {
    clients: HashSet<NodeID>,
    client_messages: HashMap<NodeID, Vec<Sphinx>>,
}

impl ProviderStorage {
    pub fn new(clients: HashSet<NodeID>, client_messages: HashMap<NodeID, Vec<Sphinx>>) -> Self {
        ProviderStorage {
            clients,
            client_messages,
        }
    }

    pub fn default() -> Self {
        ProviderStorage {
            clients: HashSet::new(),
            client_messages: HashMap::new(),
        }
    }

    /// given path_length 3: our node either 3, 4, or 5
    pub fn default_with_path_length(our_node_id: u32, path_length: usize) -> Self {
        if our_node_id < path_length as u32 || our_node_id >= (path_length * 2) as u32 {
            panic!("Our node id must be between the path length and 2 times the path length");
        }

        let mut clients = HashSet::new();
        clients.insert(NodeID::from(our_node_id - path_length as u32));

        ProviderStorage {
            clients,
            client_messages: HashMap::new(),
        }
    }
}

pub struct LoopixStorage {
    pub network_storage: Arc<RwLock<NetworkStorage>>,
    pub client_storage: Arc<RwLock<Option<ClientStorage>>>,
    pub provider_storage: Arc<RwLock<Option<ProviderStorage>>>,
}

impl LoopixStorage {
    // TODO figure out how to serialize this
    // pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
    //     serde_yaml::to_string::<LoopixStorageSave>(&LoopixStorageSave::V1((*self).clone()))
    // }

    pub async fn get_our_id(&self) -> NodeID {
        self.network_storage.read().await.node_id.clone()
    }

    pub async fn get_private_key(&self) -> StaticSecret {
        self.network_storage.read().await.private_key.clone()
    }

    pub async fn get_public_key(&self) -> PublicKey {
        self.network_storage.read().await.public_key.clone()
    }

    pub async fn get_mixes(&self) -> Vec<Vec<NodeID>> {
        self.network_storage.read().await.mixes.clone()
    }

    pub async fn set_mixes(&self, new_mixes: Vec<Vec<NodeID>>) {
        self.network_storage.write().await.mixes = new_mixes;
    }

    pub async fn get_providers(&self) -> Vec<NodeID> {
        self.network_storage.read().await.providers.clone()
    }

    pub async fn set_providers(&self, new_providers: Vec<NodeID>) {
        self.network_storage.write().await.providers = new_providers;
    }

    pub async fn get_node_public_keys(&self) -> HashMap<NodeID, PublicKey> {
        self.network_storage.read().await.node_public_keys.clone()
    }

    pub async fn set_node_public_keys(&self, new_keys: HashMap<NodeID, PublicKey>) {
        self.network_storage.write().await.node_public_keys = new_keys;
    }

    pub async fn get_our_provider(&self) -> Option<NodeID> {
        if let Some(storage) = self.client_storage.read().await.as_ref() {
            storage.our_provider.clone()
        } else {
            panic!("Client storage not found");
        }
    }

    pub async fn set_our_provider(&self, provider: Option<NodeID>) {
        if let Some(storage) = &mut *self.client_storage.write().await {
            storage.our_provider = provider;
        } else {
            panic!("Client storage not found");
        }
    }

    pub async fn get_client_to_provider_map(&self) -> HashMap<NodeID, NodeID> {
        if let Some(storage) = self.client_storage.read().await.as_ref() {
            storage.client_to_provider_map.clone()
        } else {
            panic!("Client storage not found");
        }
    }

    pub async fn update_client_provider_mapping(
        &self,
        client_id: NodeID,
        new_provider_id: NodeID,
    ) {
        if let Some(storage) = &mut *self.client_storage.write().await {
            storage.client_to_provider_map.insert(client_id, new_provider_id);
        } else {
            panic!("Client storage not found");
        }
    }

    pub async fn set_client_provider_map(&self, new_map: HashMap<NodeID, NodeID>) {
        if let Some(storage) = &mut *self.client_storage.write().await {
            storage.client_to_provider_map = new_map;
        } else {
            panic!("Client storage not found");
        }
    }

    pub async fn get_clients(&self) -> HashSet<NodeID> {
        if let Some(storage) = self.provider_storage.read().await.as_ref() {
            storage.clients.clone()
        } else {
            panic!("Provider storage not found");
        }
    }

    pub async fn add_client(&self, client_id: NodeID) {
        if let Some(storage) = &mut *self.provider_storage.write().await {
            storage.clients.insert(client_id);
        } else {
            panic!("Provider storage not found");
        }
    }

    pub async fn set_clients(&self, new_clients: HashSet<NodeID>) {
        if let Some(storage) = &mut *self.provider_storage.write().await {
            storage.clients = new_clients;
        } else {
            panic!("Provider storage not found");
        }
    }

    pub async fn get_client_messages(&self, node_id: NodeID) -> Vec<Sphinx> {
        if let Some(storage) = self.provider_storage.read().await.as_ref() {
            storage.client_messages.get(&node_id).cloned().unwrap_or_default()
        } else {
            panic!("Provider storage not found");
        }
    }

    pub async fn add_client_message(&self, client_id: NodeID, new_message: Sphinx) {
        if let Some(storage) = &mut *self.provider_storage.write().await {
            storage.client_messages.entry(client_id).or_insert(Vec::new()).push(new_message);
        } else {
            panic!("Provider storage not found");
        }
    }

}

#[derive(Clone, Serialize, Deserialize)]
struct SerializableLoopixStorage {
    pub network_storage: NetworkStorage,
    pub client_storage: Option<ClientStorage>,
    pub provider_storage: Option<ProviderStorage>,
}

impl Serialize for LoopixStorage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let serde_struct = SerializableLoopixStorage {
            network_storage: self.network_storage.blocking_read().clone(),
            client_storage: self.client_storage.blocking_read().clone(),
            provider_storage: self.provider_storage.blocking_read().clone(),
        };

        serde_struct.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for LoopixStorage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let serde_struct = SerializableLoopixStorage::deserialize(deserializer)?;
        
        Ok(LoopixStorage {
            network_storage: Arc::new(RwLock::new(serde_struct.network_storage)),
            client_storage: Arc::new(RwLock::new(serde_struct.client_storage)),
            provider_storage: Arc::new(RwLock::new(serde_struct.provider_storage)),
        })
    }
}

impl Default for LoopixStorage {
    fn default() -> Self {
        let (public_key, private_key) = Self::generate_key_pair();
        LoopixStorage {
            network_storage: Arc::new(RwLock::new(NetworkStorage {
                node_id: NodeID::rnd(),
                private_key,
                public_key,
                mixes: Vec::new(),
                providers: Vec::new(),
                node_public_keys: HashMap::new(),
            })),
            client_storage: Arc::new(RwLock::new(Option::None)),
            provider_storage: Arc::new(RwLock::new(Option::None)),
        }
    }
}

impl LoopixStorage {
    pub fn new(
        node_id: NodeID,
        private_key: StaticSecret,
        public_key: PublicKey,
        mixes: Vec<Vec<NodeID>>,
        providers: Vec<NodeID>,
        node_public_keys: HashMap<NodeID, PublicKey>,
        client_storage: Option<ClientStorage>,
        provider_storage: Option<ProviderStorage>,
    ) -> Self {
        LoopixStorage {
            network_storage: Arc::new(RwLock::new(NetworkStorage {
                node_id,
                private_key,
                public_key,
                mixes,
                providers,
                node_public_keys,
            })),
            client_storage: Arc::new(RwLock::new(client_storage)),
            provider_storage: Arc::new(RwLock::new(provider_storage)),
        }
    }

    pub fn new_with_key_generation(
        node_id: NodeID,
        mixes: Vec<Vec<NodeID>>,
        providers: Vec<NodeID>,
        node_public_keys: HashMap<NodeID, PublicKey>,
        client_storage: Option<ClientStorage>,
        provider_storage: Option<ProviderStorage>,
    ) -> Self {
        let (public_key, private_key) = Self::generate_key_pair();
        LoopixStorage {
            network_storage: Arc::new(RwLock::new(NetworkStorage {
                node_id,
                private_key,
                public_key,
                mixes,
                providers,
                node_public_keys,
            })),
            client_storage: Arc::new(RwLock::new(client_storage)),
            provider_storage: Arc::new(RwLock::new(provider_storage)),
        }
    }

    pub fn generate_key_pair() -> (PublicKey, StaticSecret) {
        let rng = rand::thread_rng();
        let private_key = StaticSecret::random_from_rng(rng);
        let public_key = PublicKey::from(&private_key);
        (public_key, private_key)
    }

    /// default testing storage with a given path length
    /// 
    /// given path_length 3:
    /// 0 to 3 are reserved for clients
    /// 3 to 6 are providers
    /// 6 to 14 are mixes
    pub fn default_with_path_length(node_id: u32, path_length: usize, private_key: StaticSecret, public_key: PublicKey, client_storage: Option<ClientStorage>, provider_storage: Option<ProviderStorage>) -> Self {
        //provider generation
        let providers = NodeIDs::new_range(path_length as u32, (path_length * 2) as u32).to_vec();

        //mix generation
        let mut mixes = Vec::new();
        for i in (path_length * 2..path_length * 2 + path_length * path_length).step_by(path_length as usize) {
            mixes.push(NodeIDs::new_range(i as u32, (i + path_length) as u32).to_vec());
        }

        LoopixStorage {
            network_storage: Arc::new(RwLock::new(NetworkStorage {
                node_id: NodeID::from(node_id),
                private_key,
                public_key,
                mixes,
                providers,
                node_public_keys: HashMap::new(),
            })),
            client_storage: Arc::new(RwLock::new(client_storage)),
            provider_storage: Arc::new(RwLock::new(provider_storage)),
        }
    }

}

// region: Derived functions
impl PartialEq for LoopixStorage {
    fn eq(&self, other: &Self) -> bool {
        let self_network_storage = self.network_storage.blocking_read();
        let other_network_storage = other.network_storage.blocking_read();

        let self_client_storage = self.client_storage.blocking_read();
        let other_client_storage = other.client_storage.blocking_read();

        let self_provider_storage = self.provider_storage.blocking_read();
        let other_provider_storage = other.provider_storage.blocking_read();

        self_network_storage.node_id == other_network_storage.node_id
            && self_network_storage.private_key.to_bytes() == other_network_storage.private_key.to_bytes()
            && self_network_storage.public_key == other_network_storage.public_key
            && self_network_storage.mixes == other_network_storage.mixes
            && self_network_storage.providers == other_network_storage.providers
            && self_network_storage.node_public_keys == other_network_storage.node_public_keys
            && *self_client_storage == *other_client_storage
            && *self_provider_storage == *other_provider_storage
    }
}

impl std::fmt::Debug for LoopixStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let network_storage = self.network_storage.blocking_read();
        let client_storage = self.client_storage.blocking_read();
        let provider_storage = self.provider_storage.blocking_read();

        f.debug_struct("LoopixStorage")
            .field("node_id", &network_storage.node_id)
            .field("private_key", &"[hidden]")
            .field("public_key", &network_storage.public_key)
            .field("mixes", &network_storage.mixes)
            .field("providers", &network_storage.providers)
            .field("node_public_keys", &network_storage.node_public_keys)
            .field("client_storage", &*client_storage)
            .field("provider_storage", &*provider_storage)
            .finish()
    }
}
// endregion: Derived functions

// region: Serde functions
pub fn serialize_public_key<S>(key: &PublicKey, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let key_bytes: &[u8] = key.as_bytes();
    serializer.serialize_bytes(key_bytes)
}

pub fn deserialize_public_key<'de, D>(deserializer: D) -> Result<PublicKey, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let key_bytes: [u8; 32] = serde::Deserialize::deserialize(deserializer)?;
    Ok(PublicKey::from(key_bytes))
}

pub fn serialize_static_secret<S>(key: &StaticSecret, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let key_bytes: [u8; 32] = key.to_bytes();
    serializer.serialize_bytes(&key_bytes)
}

pub fn deserialize_static_secret<'de, D>(deserializer: D) -> Result<StaticSecret, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let key_bytes: [u8; 32] = serde::Deserialize::deserialize(deserializer)?;
    Ok(StaticSecret::from(key_bytes))
}

pub fn serialize_node_public_keys<S>(
    keys: &HashMap<NodeID, PublicKey>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let keys_bytes: HashMap<_, _> = keys
        .iter()
        .map(|(node_id, key)| (node_id, key.as_bytes()))
        .collect();
    keys_bytes.serialize(serializer)
}

pub fn deserialize_node_public_keys<'de, D>(
    deserializer: D,
) -> Result<HashMap<NodeID, PublicKey>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let keys_bytes: HashMap<NodeID, [u8; 32]> = HashMap::deserialize(deserializer)?;
    let keys = keys_bytes
        .into_iter()
        .map(|(node_id, key_bytes)| (node_id, PublicKey::from(key_bytes)))
        .collect();
    Ok(keys)
}

// endregion: Serde functions

impl Clone for LoopixStorage {
    fn clone(&self) -> Self {
        LoopixStorage {
            network_storage: Arc::new(RwLock::new(self.network_storage.blocking_read().clone())),
            client_storage: Arc::new(RwLock::new(self.client_storage.blocking_read().clone())),
            provider_storage: Arc::new(RwLock::new(self.provider_storage.blocking_read().clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_default_with_path_length() {
        let path_length = 3;
        let node_id = 1;
        let client_storage = None;
        let provider_storage = None;

        let (public_key, private_key) = LoopixStorage::generate_key_pair();

        let storage = LoopixStorage::default_with_path_length(
            node_id,
            path_length,
            private_key,
            public_key,
            client_storage,
            provider_storage,
        );

        let network_storage = storage.network_storage.read().await;
        println!("Providers: {:?}", network_storage.providers);
        println!("Mixes: {:?}", network_storage.mixes);
        println!("NodeId: {:?}", network_storage.node_id);
    }

    #[test]
    fn test_client_storage_default_with_path_length() {
        let our_node_id = 1;
        let path_length = 3;

        let client_storage = ClientStorage::default_with_path_length(our_node_id, path_length);

        println!("Our Provider: {:?}", client_storage.our_provider);
        println!("Client to Provider Map: {:?}", client_storage.client_to_provider_map);
    }

    #[test]
    fn test_provider_storage_default_with_path_length() {
        let our_node_id = 4;
        let path_length = 3;

        let provider_storage = ProviderStorage::default_with_path_length(our_node_id, path_length);

        println!("Clients: {:?}", provider_storage.clients);
    }

    #[test]
    fn test_loopix_storage_serde() {
        let path_length = 3;
        let node_id = 1;
        let (public_key, private_key) = LoopixStorage::generate_key_pair();

        let mut node_public_keys = HashMap::new();
        node_public_keys.insert(NodeID::from(1), public_key);

        let client_storage = Some(ClientStorage::default_with_path_length(node_id, path_length));
        let provider_storage = None;

        let original_storage = LoopixStorage::default_with_path_length(
            node_id,
            path_length,
            private_key,
            public_key,
            client_storage,
            provider_storage,
        );

        let serialized = serde_yaml::to_string(&original_storage).expect("Failed to serialize");
        let deserialized: LoopixStorage = serde_yaml::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(original_storage, deserialized);
    }
}

