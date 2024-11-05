use std::time::Duration;

use crate::loopix::storage::LoopixStorage;
use crate::loopix::config::CoreConfig;
use crate::loopix::core::LoopixCore;

use flarch::nodeids::NodeID;
use sphinx_packet::header::delays::Delay;
use sphinx_packet::payload::Payload;
use sphinx_packet::SphinxPacket;
use crate::loopix::sphinx::Sphinx;
use crate::overlay::messages::NetworkWrapper;
use crate::loopix::messages::{MessageType};
use rand::seq::SliceRandom;
use crate::loopix::broker::MODULE_NAME;

#[derive(Debug, Clone, PartialEq)]
pub struct Client {
    storage: LoopixStorage,
    config: CoreConfig,
}

impl Client {
    pub fn new(storage: LoopixStorage, config: CoreConfig) -> Self {
        Client {
            storage,
            config,
        }
    }
}

impl LoopixCore for Client {
    fn get_config(&self) -> &CoreConfig {
        &self.config
    }

    fn get_storage(&self) -> &LoopixStorage {
        &self.storage
    }

    async fn get_our_id(&self) -> NodeID {
        self.storage.get_our_id().await
    }

    /// Packet with our provider as destination (loop)
    async fn create_loop_message(&self) -> (NodeID, Sphinx) {
        // get our provider
        let our_provider = self.get_our_provider().await;

        // create route
        let route = self.create_route(self.get_config().path_length(), our_provider, None, our_provider).await;
        
        // create the networkmessage
        let loop_msg = serde_yaml::to_string(&MessageType::Loop).unwrap();
        let msg = NetworkWrapper{ module: MODULE_NAME.into(), msg: loop_msg};

        // create sphinx packet
        let our_provider = self.get_our_provider().await.unwrap();
        let (_, sphinx) = self.create_sphinx_packet(our_provider, msg, &route);
        (our_provider, sphinx)
    }

    /// Packet with a random provider as destination (drop)
    async fn create_drop_message(&self) -> (NodeID, Sphinx) {
        // pick random provider
        let providers = self.get_storage().get_providers().await;
        let random_provider = providers.choose(&mut rand::thread_rng()).unwrap();

        // get our provider
        let our_provider = self.get_our_provider().await;

        // create route
        let route = self.create_route(self.get_config().path_length(), our_provider, None, Some(*random_provider)).await;

        // create the networkmessage
        let drop_msg = serde_yaml::to_string(&MessageType::Drop).unwrap();
        let msg = NetworkWrapper{ module: MODULE_NAME.into(), msg: drop_msg};

        // create sphinx packet
        let (_, sphinx) = self.create_sphinx_packet(*random_provider, msg, &route);
        (*random_provider, sphinx)
    }
}



pub trait ClientInterface {
    async fn register_provider(&mut self, provider: NodeID);
    async fn get_provider(&self) -> Option<NodeID>;

    async fn create_pull_message(&self) -> (NodeID, Sphinx);
    async fn create_subscribe_message(&self) -> (NodeID, Sphinx);
    async fn create_payload_message(&self, destination: NodeID, msg: NetworkWrapper) -> (NodeID, Sphinx);
}

impl Client {

    pub async fn register_provider(&mut self, provider: NodeID) {
        // get IDs
        let our_id = self.get_storage().get_our_id().await;
        let providers = self.get_storage().get_providers().await;

        // check if provider is valid
        if provider == our_id {
            panic!("Provider ID cannot be the same as our ID");
        }
        if !providers.contains(&provider) {
            panic!("Provider ID is not in the list of providers");
        }

        // set provider
        self.storage.set_our_provider(Some(provider)).await;
    }

    pub async fn get_our_provider(&self) -> Option<NodeID> {
        self.storage.get_our_provider().await
    }

    pub async fn create_pull_message(&self) -> (NodeID, Option<Sphinx>) {
        let our_id = self.get_our_id().await;

        let provider = match self.get_our_provider().await {
            Some(provider) => provider,
            None => {
                let providers = self.get_storage().get_providers().await;
                if providers.is_empty() {
                    log::error!("No providers available");
                    return (our_id, None);
                }
                *providers.choose(&mut rand::thread_rng()).unwrap()
            }
        };

        //create route (path length is 0 because we directly send to the provider)
        let route = self.create_route(0, None, None, Some(provider)).await;

        // create the networkmessage
        let pull_msg = serde_yaml::to_string(&MessageType::PullRequest(our_id)).unwrap();
        let msg = NetworkWrapper{ module: MODULE_NAME.into(), msg: pull_msg};
        
        // create sphinx packet
        let (_, sphinx) = self.create_sphinx_packet(provider, msg, &route);
        (provider, Some(sphinx))
    }

    pub async fn create_subscribe_message(&self) -> (NodeID, Sphinx) {
        let our_id = self.get_our_id().await;

        // get provider or choose one randomly
        let provider = match self.get_our_provider().await {
            Some(provider) => provider,
            None => {
                let providers = self.get_storage().get_providers().await;
                *providers.choose(&mut rand::thread_rng()).unwrap()
            }
        };

        // create route (path length is 0 because we directly send to the provider)
        let route = self.create_route(0, None, None, Some(provider)).await;

        // create the networkmessage
        let subscribe_msg = serde_yaml::to_string(&MessageType::SubscriptionRequest(our_id)).unwrap();
        let msg = NetworkWrapper{ module: MODULE_NAME.into(), msg: subscribe_msg};
        
        // create sphinx packet
        let (_, sphinx) = self.create_sphinx_packet(provider, msg, &route);
        (provider, sphinx)
    }

    pub async fn process_overlay_message(&self, node_id: NodeID, message: NetworkWrapper) -> (NodeID, Sphinx) {
        let (next_node, sphinx) = self.create_payload_message(node_id, message).await;
        (next_node, sphinx)
    }

    pub async fn create_payload_message(&self, destination: NodeID, msg: NetworkWrapper) -> (NodeID, Sphinx) {
        // get provider for destination
        let client_to_provider_map = self.get_storage().get_client_to_provider_map().await;
        let dst_provider = client_to_provider_map.get(&destination).unwrap();

        // get our id and provider
        let our_id = self.get_our_id().await;
        let our_provider = self.get_our_provider().await;

        // create route 
        let route = self.create_route(self.get_config().path_length(), our_provider, Some(*dst_provider), Some(destination)).await;

        // create payload message
        let payload_msg = serde_yaml::to_string(&MessageType::Payload(our_id, msg)).unwrap();
        let network_msg = NetworkWrapper{ module: MODULE_NAME.into(), msg: payload_msg};
        
        // create sphinx packet
        let (_, sphinx) = self.create_sphinx_packet(destination, network_msg, &route);
        (*dst_provider, sphinx)
    }

    pub async fn process_final_hop(&self, destination: NodeID, surb_id: [u8; 16], payload: Payload) {
        todo!("Client hasnt implemented process_final_hop yet")
    }

}

#[cfg(test)]
mod tests {
    use crate::loopix::config::LoopixConfig;
    use crate::loopix::config::LoopixRole;
    use std::collections::HashMap;
    use flarch::nodeids::NodeIDs;

    use super::*;

    async fn setup() -> (Client, u32, usize) {
        let path_length = 2;
        let mut node_public_keys = HashMap::new();
        let mut node_key_pairs = HashMap::new();

        for mix in 0..path_length*path_length + path_length {
            for node_id in NodeIDs::new_range(mix as u32, (mix + path_length) as u32).to_vec() {
                let (public_key, private_key) = LoopixStorage::generate_key_pair();
                node_public_keys.insert(node_id, public_key);
                node_key_pairs.insert(node_id, (public_key, private_key));
            }
        }

        let node_id = 0;
        let private_key = &node_key_pairs.get(&NodeID::from(node_id)).unwrap().1;
        let public_key = &node_key_pairs.get(&NodeID::from(node_id)).unwrap().0;

        let config = LoopixConfig::default_with_path_length(
            LoopixRole::Client,
            node_id,
            path_length,
            private_key.clone(),
            public_key.clone(),
        );

        config.storage_config.set_node_public_keys(node_public_keys).await;

        (Client::new(config.storage_config, config.core_config), node_id, path_length)
    }

    #[tokio::test]
    async fn get_provider() {
        let (client, node_id, path_length) = setup().await;
        assert_eq!(client.get_our_provider().await, Some(NodeID::from(node_id + path_length as u32)));
    }

    #[tokio::test]
    async fn register_provider() {
        let (mut client, _, path_length) = setup().await;
        let new_provider = path_length as u32 + rand::random::<u32>() % (path_length * 2 - path_length) as u32;          
        println!("New provider ID: {}", new_provider);
        client.register_provider(NodeID::from(new_provider)).await;
        assert_eq!(client.get_our_provider().await, Some(NodeID::from(new_provider)));
    }


}
