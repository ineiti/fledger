use crate::loopix::config::CoreConfig;
use crate::loopix::core::LoopixCore;
use crate::loopix::storage::LoopixStorage;

use crate::loopix::broker::MODULE_NAME;
use crate::loopix::messages::MessageType;
use crate::loopix::sphinx::Sphinx;
use crate::overlay::messages::NetworkWrapper;
use async_trait::async_trait;
use flarch::nodeids::NodeID;
use sphinx_packet::header::delays::Delay;
use sphinx_packet::payload::Payload;
use sphinx_packet::SphinxPacket;

#[derive(Debug, Clone, PartialEq)]
pub struct Client {
    storage: LoopixStorage,
    config: CoreConfig,
}

impl Client {
    pub fn new(storage: LoopixStorage, config: CoreConfig) -> Self {
        Client { storage, config }
    }
}

#[async_trait]
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
        let route = self
            .create_route(
                self.get_config().path_length(),
                our_provider,
                None,
                our_provider,
            )
            .await;

        // create the networkmessage
        
        let loop_msg = serde_yaml::to_string(&MessageType::Loop).unwrap();
        let msg = NetworkWrapper {
            module: MODULE_NAME.into(),
            msg: loop_msg,
        };

        // create sphinx packet
        let our_provider = self.get_our_provider().await.unwrap();
        let (_, sphinx) = self.create_sphinx_packet(our_provider, msg, &route);
        (our_provider, sphinx)
    }

    /// Packet with a random provider as destination (drop)
    async fn create_drop_message(&self) -> (NodeID, Sphinx) {
        // pick random provider
        let random_provider = self.get_storage().get_random_provider().await;

        // get our provider
        let our_provider = self.get_our_provider().await;

        // create route
        let route = self
            .create_route(
                self.get_config().path_length(),
                our_provider,
                None,
                Some(random_provider),
            )
            .await;

        // create the networkmessage
        let drop_msg = serde_yaml::to_string(&MessageType::Drop).unwrap();
        let msg = NetworkWrapper {
            module: MODULE_NAME.into(),
            msg: drop_msg,
        };

        // create sphinx packet
        let (_, sphinx) = self.create_sphinx_packet(random_provider, msg, &route);
        (random_provider, sphinx)
    }

    async fn process_final_hop(
        &self,
        destination: NodeID,
        _surb_id: [u8; 16],
        payload: Payload,
    ) -> (NodeID, Option<NetworkWrapper>, Option<Vec<(Delay, Sphinx)>>, Option<MessageType>) {

        if destination != self.get_our_id().await {
            log::info!("Final hop received, but we're not the destination");
            return (destination, None, None, None);
        }

        let plaintext = payload.recover_plaintext().unwrap();
        let plaintext_str = std::str::from_utf8(&plaintext).unwrap();

        if let Ok(module_message) = serde_yaml::from_str::<NetworkWrapper>(plaintext_str) {
            if module_message.module == MODULE_NAME {
                if let Ok(message) = serde_yaml::from_str::<MessageType>(&module_message.msg) {
                    match message.clone() {
                        MessageType::Payload(source, msg) => {
                            (source, Some(msg), None, Some(message))
                        }
                        MessageType::PullRequest(_) => { log::error!("Client shouldn't receive pull requests!"); (destination, None, None, Some(message)) },
                        MessageType::SubscriptionRequest(_) => { log::error!("Client shouldn't receive subscription requests!"); (destination, None, None, Some(message)) },
                        MessageType::Drop => { log::info!("Client received drop"); (destination, None, None, Some(message)) },
                        MessageType::Loop => { log::info!("Client received loop"); (destination, None, None, Some(message)) },
                        MessageType::Dummy => { log::info!("Client received dummy"); (destination, None, None, Some(message)) },
                    }
                } else {
                    log::error!("Received message in wrong format");
                    (destination, None, None, None)
                }
            } else {
                log::error!("Received message from module that is not Loopix: {:?}", module_message.module);
                (destination, None, None, None)
            }
        } else {
            log::error!("Could not recover plaintext");
            (destination, None, None, None)
        }
    }

    async fn process_forward_hop(
        &self,
        _next_packet: Box<SphinxPacket>,
        _next_address: NodeID,
        delay: Delay,
    ) -> (NodeID, Delay, Option<Sphinx>) {
        log::error!("Client shouldn't receive forward hops");
        let our_id = self.get_our_id().await;
        (our_id, delay, None)
    }
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
                self.get_storage().get_random_provider().await
            }
        };

        //create route (path length is 0 because we directly send to the provider)
        let route = self.create_route(0, None, None, Some(provider)).await;

        // create the networkmessage
        let pull_msg = serde_yaml::to_string(&MessageType::PullRequest(our_id)).unwrap();
        let msg = NetworkWrapper {
            module: MODULE_NAME.into(),
            msg: pull_msg,
        };

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
                self.get_storage().get_random_provider().await
            }
        };

        // create route (path length is 0 because we directly send to the provider)
        let route = self.create_route(0, None, None, Some(provider)).await;

        // create the networkmessage
        let subscribe_msg =
            serde_yaml::to_string(&MessageType::SubscriptionRequest(our_id)).unwrap();
        let msg = NetworkWrapper {
            module: MODULE_NAME.into(),
            msg: subscribe_msg,
        };

        // create sphinx packet
        let (_, sphinx) = self.create_sphinx_packet(provider, msg, &route);
        (provider, sphinx)
    }

    pub async fn process_overlay_message(
        &self,
        node_id: NodeID,
        message: NetworkWrapper,
    ) -> (NodeID, Sphinx) {
        let (next_node, sphinx) = self.create_payload_message(node_id, message).await;
        log::trace!("Next node: {:?}", next_node);
        log::trace!("Destination: {:?}", node_id);
        let our_id = self.get_our_id().await;
        let our_provider = self.get_our_provider().await;
        log::trace!("Our ID: {:?}, Our Provider: {:?}", our_id, our_provider);
        (next_node, sphinx)
    }

    pub async fn create_payload_message(
        &self,
        destination: NodeID,
        msg: NetworkWrapper,
    ) -> (NodeID, Sphinx) {
        // get provider for destination
        let client_to_provider_map = self.get_storage().get_client_to_provider_map().await;
        let dst_provider = client_to_provider_map.get(&destination).unwrap();

        // get our id and provider
        let our_id = self.get_our_id().await;
        let our_provider = self.get_our_provider().await;

        // create route
        let route = self
            .create_route(
                self.get_config().path_length(),
                our_provider,
                Some(*dst_provider),
                Some(destination),
            )
            .await;

        // create payload message
        let payload_msg = serde_yaml::to_string(&MessageType::Payload(our_id, msg)).unwrap();
        let network_msg = NetworkWrapper {
            module: MODULE_NAME.into(),
            msg: payload_msg,
        };

        // create sphinx packet
        let (_, sphinx) = self.create_sphinx_packet(destination, network_msg, &route);
        (our_provider.unwrap(), sphinx)
    }

}

#[cfg(test)]
mod tests {
    use crate::loopix::config::LoopixConfig;
    use crate::loopix::config::LoopixRole;
    use crate::loopix::testing::LoopixSetup;
    use crate::nodeconfig::NodeInfo;

    use super::*;

    async fn setup() -> (Client, Vec<NodeInfo>, usize) {
        let path_length = 2;
        let (all_nodes, node_public_keys, loopix_key_pairs, _) = LoopixSetup::create_nodes_and_keys(path_length);

        // get our node info
        let node_id = all_nodes.iter().next().unwrap().get_id();
        let private_key = &loopix_key_pairs.get(&NodeID::from(node_id)).unwrap().1;
        let public_key = &loopix_key_pairs.get(&NodeID::from(node_id)).unwrap().0;

        let config = LoopixConfig::default_with_path_length(
            LoopixRole::Client,
            node_id,
            path_length,
            private_key.clone(),
            public_key.clone(),
            all_nodes.clone(),
        );

        config
            .storage_config
            .set_node_public_keys(node_public_keys)
            .await;

        (
            Client::new(config.storage_config, config.core_config),
            all_nodes.clone(),
            path_length,
        )
    }

    #[tokio::test]
    async fn get_provider() {
        let (client, nodes, path_length) = setup().await;
        let node_id = client.get_our_id().await;
        let our_id_index = nodes.iter().position(|node| node.get_id() == node_id).unwrap();
        assert_eq!(
            client.get_our_provider().await,
            Some(NodeID::from(nodes[our_id_index + path_length].get_id()))
        );
    }

    #[tokio::test]
    async fn register_provider() {
        let (mut client, nodes, path_length) = setup().await;
        let new_provider = nodes[path_length].get_id();
        println!("New provider ID: {}", new_provider);
        client.register_provider(new_provider).await;
        assert_eq!(
            client.get_our_provider().await,
            Some(new_provider)
        );
    }
}
