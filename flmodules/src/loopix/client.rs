use std::sync::Arc;
use std::time::{Instant, SystemTime};

use crate::loopix::config::CoreConfig;
use crate::loopix::core::LoopixCore;
use crate::loopix::storage::LoopixStorage;

use crate::loopix::broker::MODULE_NAME;
use crate::loopix::messages::MessageType;
use crate::loopix::sphinx::Sphinx;
use crate::overlay::messages::NetworkWrapper;
use async_trait::async_trait;
use flarch::nodeids::NodeID;
use sphinx_packet::payload::Payload;
use sphinx_packet::SphinxPacket;

use super::ENCRYPTION_LATENCY;

#[derive(Debug, PartialEq)]
pub struct Client {
    storage: Arc<LoopixStorage>,
    config: CoreConfig,
}

impl Client {
    pub fn new(storage: Arc<LoopixStorage>, config: CoreConfig) -> Self {
        Client { storage, config }
    }
}

#[async_trait]
impl LoopixCore for Client {
    fn get_config(&self) -> &CoreConfig {
        &self.config
    }

    fn get_storage(&self) -> &Arc<LoopixStorage> {
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
        // As a client, the node routes it's loop message through its own provider
        // since this is a loop message, the node sets the destination to it's own ID
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
        // self.storage
        //     .add_sent_message(route, MessageType::Loop, sphinx.message_id.clone())
        //     .await; // TODO uncomment
        (our_provider, sphinx)
    }

    async fn process_final_hop(
        &self,
        destination: NodeID,
        _surb_id: [u8; 16],
        payload: Payload,
    ) -> (
        NodeID,
        Option<NetworkWrapper>,
        Option<(NodeID, Vec<(Sphinx, Option<SystemTime>)>)>,
        Option<MessageType>,
    ) {
        if destination != self.get_our_id().await {
            log::info!("Final hop received, but we're not the destination");
            return (destination, None, None, None);
        }

        let plaintext = match payload.recover_plaintext() {
            Ok(plaintext) => plaintext,
            Err(e) => {
                log::error!("Failed to recover plaintext: {:?}", e);
                return (destination, None, None, None);
            }
        };
        let plaintext_str = match std::str::from_utf8(&plaintext) {
            Ok(plaintext_str) => plaintext_str,
            Err(e) => {
                log::error!("Failed to convert plaintext to string: {:?}", e);
                return (destination, None, None, None);
            }
        };

        if let Ok(module_message) = serde_yaml::from_str::<NetworkWrapper>(plaintext_str) {
            if module_message.module == MODULE_NAME {
                if let Ok(message) = serde_yaml::from_str::<MessageType>(&module_message.msg) {
                    match message.clone() {
                        MessageType::Payload(source, msg) => {
                            log::info!("Client received payload from {}: {:?}", source, msg);
                            (source, Some(msg), None, Some(message))
                        }
                        MessageType::PullRequest(client_id) => {
                            log::warn!("Client shouldn't receive pull requests!");
                            (client_id, None, None, Some(message))
                        }
                        MessageType::SubscriptionRequest(client_id) => {
                            log::warn!("Client shouldn't receive subscription requests!");
                            (client_id, None, None, Some(message))
                        }
                        MessageType::Drop => {
                            log::warn!("Client received drop");
                            (destination, None, None, Some(message))
                        }
                        MessageType::Loop => {
                            log::warn!("Client received loop");
                            (destination, None, None, Some(message))
                        }
                        MessageType::Dummy => {
                            log::trace!("Client received dummy");
                            (destination, None, None, Some(message))
                        }
                    }
                } else {
                    log::error!("Received message in wrong format");
                    (destination, None, None, None)
                }
            } else {
                log::error!(
                    "Received message from module that is not Loopix: {:?}",
                    module_message.module
                );
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
        message_id: String,
    ) -> (NodeID, Option<Sphinx>) {
        log::error!(
            "Client shouldn't receive forward hops, message id: {}",
            message_id
        );
        let our_id = self.get_our_id().await;
        (our_id, None)
    }
}

impl Client {
    pub async fn async_clone(&self) -> Self {
        let storage_clone = Arc::new(self.storage.async_clone().await);

        Client {
            storage: storage_clone,
            config: self.config.clone(),
        }
    }

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

    /// Packet with a random provider as destination (drop)
    pub async fn create_drop_message(&self) -> (NodeID, Sphinx) {
        // pick random provider
        let random_provider = self.get_storage().get_random_provider().await;

        // get our provider
        let our_provider = match self.get_our_provider().await {
            Some(provider) => provider,
            None => {
                log::error!("Our provider is None");
                NodeID::from(1)
            }
        };

        // create route
        // As a client, the node routes it's drop message through its own provider
        // it chooses a random provider as the destination
        let route = self
            .create_route(
                self.get_config().path_length(),
                Some(our_provider),
                Some(random_provider),
                None,
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
        // self.storage
        //     .add_sent_message(route, MessageType::Drop, sphinx.message_id.clone())
        //     .await; // TODO uncomment

        (our_provider, sphinx)
    }

    pub async fn create_pull_message(self) -> (NodeID, Option<Sphinx>) {
        let our_id = self.get_our_id().await;

        let our_provider = self.get_our_provider().await;

        if our_provider.is_none() {
            log::error!("Client has no provider");
            return (our_id, None);
        }

        let provider = our_provider.unwrap();

        //create route (path length is 0 because we directly send to the provider)
        // there only the destination, which is the the nodes' own provider
        let route = self.create_route(0, None, None, Some(provider)).await;

        // create the networkmessage
        let pull_msg = serde_yaml::to_string(&MessageType::PullRequest(our_id)).unwrap();
        let msg = NetworkWrapper {
            module: MODULE_NAME.into(),
            msg: pull_msg,
        };

        // create sphinx packet
        let (_, sphinx) = self.create_sphinx_packet(provider, msg, &route);
        // self.storage
        //     .add_sent_message(route, MessageType::PullRequest(our_id), sphinx.message_id.clone())
        //     .await; // TODO uncomment
        (provider, Some(sphinx))
    }

    pub async fn create_subscribe_message(&mut self) -> (NodeID, Sphinx) {
        let our_id = self.get_our_id().await;

        // get provider or choose one randomly
        let provider = match self.get_our_provider().await {
            Some(provider) => provider,
            None => {
                let provider = self.get_storage().get_random_provider().await;
                self.register_provider(provider).await;
                provider
            }
        };

        // create route (path length is 0 because we directly send to the provider)
        // there only the destination, which is the the nodes' chosen provider
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
        // self.storage
        //     .add_sent_message(route, MessageType::SubscriptionRequest(our_id), sphinx.message_id.clone())
        //     .await; // TODO uncomment
        (provider, sphinx)
    }

    pub async fn process_overlay_message(
        &self,
        node_id: NodeID,
        message: NetworkWrapper,
    ) -> (NodeID, Sphinx) {
        let start_time = Instant::now();
        let (next_node, sphinx) = self.create_payload_message(node_id, message).await;
        let end_time = start_time.elapsed().as_millis() as f64;
        ENCRYPTION_LATENCY.observe(end_time);
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
        // As a client, the node routes it's payload message through its own provider
        // the destination is the destination client node
        // the message is routed to the destinations provider before ending at the destination client node
        let route = self
            .create_route(
                self.get_config().path_length(),
                our_provider,
                Some(*dst_provider),
                Some(destination),
            )
            .await;

        // create payload message
        let payload_msg =
            serde_yaml::to_string(&MessageType::Payload(our_id, msg.clone())).unwrap();
        let network_msg = NetworkWrapper {
            module: MODULE_NAME.into(),
            msg: payload_msg,
        };

        // create sphinx packet
        let (_, sphinx) = self.create_sphinx_packet(destination, network_msg, &route);
        // self.storage
        //     .add_sent_message(route, MessageType::Payload(our_id, msg), sphinx.message_id.clone())
        //     .await;
        log::trace!("Sent message {} to {}", sphinx.message_id, destination);
        (our_provider.unwrap(), sphinx)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use ed25519_compact::KeyPair;
    use ed25519_compact::Seed;
    use sphinx_packet::ProcessedPacket;
    use x25519_dalek::PublicKey;
    use x25519_dalek::StaticSecret;

    use crate::loopix::config::LoopixConfig;
    use crate::loopix::config::LoopixRole;
    use crate::loopix::sphinx::node_id_from_destination_address;
    use crate::loopix::sphinx::node_id_from_node_address;
    use crate::loopix::testing::LoopixSetup;
    use crate::nodeconfig::NodeInfo;

    use super::*;

    async fn setup() -> (
        Client,
        Vec<NodeInfo>,
        usize,
        HashMap<NodeID, (PublicKey, StaticSecret)>,
        Vec<NodeInfo>,
    ) {
        let path_length = 2;
        let (all_nodes, node_public_keys, loopix_key_pairs, _) =
            LoopixSetup::create_nodes_and_keys(path_length);

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
            Client::new(Arc::new(config.storage_config), config.core_config),
            all_nodes.clone(),
            path_length,
            loopix_key_pairs,
            all_nodes.clone(),
        )
    }

    #[tokio::test]
    async fn get_provider() {
        let (client, nodes, path_length, _, _) = setup().await;
        let node_id = client.get_our_id().await;
        let our_id_index = nodes
            .iter()
            .position(|node| node.get_id() == node_id)
            .unwrap();
        assert_eq!(
            client.get_our_provider().await,
            Some(NodeID::from(nodes[our_id_index + path_length].get_id()))
        );
    }

    #[tokio::test]
    async fn register_provider() {
        let (mut client, nodes, path_length, _, _) = setup().await;
        let new_provider = nodes[path_length].get_id();
        // println!("New provider ID: {}", new_provider);
        client.register_provider(new_provider).await;
        assert_eq!(client.get_our_provider().await, Some(new_provider));
    }

    #[tokio::test]
    async fn test_deep_clone_storage() {
        let (client1, _, _, _, _) = setup().await;

        let client2: Client = client1.async_clone().await;

        let keypair = KeyPair::from_seed(Seed::default());
        client1
            .get_storage()
            .add_client_in_network(NodeInfo::new(keypair.pk))
            .await;

        let ptr1 = Arc::as_ptr(&client1.get_storage());
        let ptr2 = Arc::as_ptr(&client2.get_storage());

        assert_ne!(
            ptr1, ptr2,
            "The cloned client's storage should not point to the same memory address as the original"
        );
    }

    #[tokio::test]
    async fn test_create_payload_message() {
        let (client, _nodes, _path_length, key_pairs, _all_nodes) = setup().await;

        let dst_client = client.get_storage().get_clients_in_network().await;

        println!("Dst client: {:?}", dst_client);

        let (_next_node, sphinx) = client
            .create_payload_message(
                dst_client[0].get_id(),
                NetworkWrapper {
                    module: MODULE_NAME.into(),
                    msg: serde_yaml::to_string(&MessageType::Dummy).unwrap(),
                },
            )
            .await;

        let sent_msg = client.get_storage().get_sent_messages().await;
        println!("Sent messages: {:?}", sent_msg);

        let (_timestamp, route, _msg, _msg_id) = sent_msg[0].clone();

        let mut sphinx_packet = sphinx.clone();

        let mut next_node = route[0];

        for node_id in route {
            println!("Node ID: {}", node_id);
            assert_eq!(next_node, node_id);
            let key_pair = key_pairs.get(&node_id).unwrap();
            let static_secret = &key_pair.1;

            let processed = match sphinx_packet.clone().inner.process(&static_secret) {
                Ok(processed) => processed,
                Err(e) => {
                    log::error!("Failed to process packet: {:?}", e);
                    assert!(false);
                    return;
                }
            };

            match processed {
                ProcessedPacket::ForwardHop(next_packet, next_address, _delay) => {
                    sphinx_packet = Sphinx {
                        message_id: sphinx_packet.message_id.clone(),
                        inner: *next_packet,
                    };
                    next_node = node_id_from_node_address(next_address);
                }
                ProcessedPacket::FinalHop(destination, _, payload) => {
                    // Check if the final destination matches our ID
                    let dest = node_id_from_destination_address(destination);
                    if dest == node_id {
                        // Recover the network wrapper
                        if let Ok(module_message) = serde_yaml::from_str::<NetworkWrapper>(
                            std::str::from_utf8(&payload.recover_plaintext().unwrap()).unwrap(),
                        ) {
                            // check module name
                            assert_eq!(module_message.module, MODULE_NAME);

                            let msg =
                                serde_yaml::from_str::<MessageType>(&module_message.msg).unwrap();
                            match msg {
                                MessageType::Payload(_, _) => assert!(true),
                                _ => assert!(false),
                            }
                        } else {
                            assert!(false);
                        }
                    } else {
                        assert!(false);
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn test_create_drop_message() {
        let (client, _nodes, _path_length, key_pairs, _all_nodes) = setup().await;

        let (_next_node, sphinx) = client.create_drop_message().await;

        let sent_msg = client.get_storage().get_sent_messages().await;
        println!("Sent messages: {:?}", sent_msg);

        let (_timestamp, route, _msg, _msg_id) = sent_msg[0].clone();

        let mut sphinx_packet = sphinx.clone();

        let mut next_node = route[0];

        for node_id in route {
            println!("Node ID: {}", node_id);
            assert_eq!(next_node, node_id);
            let key_pair = key_pairs.get(&node_id).unwrap();
            let static_secret = &key_pair.1;

            let processed = match sphinx_packet.clone().inner.process(&static_secret) {
                Ok(processed) => processed,
                Err(e) => {
                    log::error!("Failed to process packet: {:?}", e);
                    assert!(false);
                    return;
                }
            };

            match processed {
                ProcessedPacket::ForwardHop(next_packet, next_address, _delay) => {
                    sphinx_packet = Sphinx {
                        message_id: sphinx_packet.message_id.clone(),
                        inner: *next_packet,
                    };
                    next_node = node_id_from_node_address(next_address);
                }
                ProcessedPacket::FinalHop(destination, _, payload) => {
                    // Check if the final destination matches our ID
                    let dest = node_id_from_destination_address(destination);
                    if dest == node_id {
                        // Recover the network wrapper
                        if let Ok(module_message) = serde_yaml::from_str::<NetworkWrapper>(
                            std::str::from_utf8(&payload.recover_plaintext().unwrap()).unwrap(),
                        ) {
                            // check module name
                            assert_eq!(module_message.module, MODULE_NAME);

                            let msg =
                                serde_yaml::from_str::<MessageType>(&module_message.msg).unwrap();
                            match msg {
                                MessageType::Drop => assert!(true),
                                _ => assert!(false),
                            }
                        } else {
                            assert!(false);
                        }
                    } else {
                        assert!(false);
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn test_create_subscribe_message() {
        let (mut client, _nodes, _path_length, key_pairs, _all_nodes) = setup().await;

        let (_next_node, sphinx) = client.create_subscribe_message().await;

        let sent_msg = client.get_storage().get_sent_messages().await;
        println!("Sent messages: {:?}", sent_msg);

        let (_timestamp, route, _msg, _msg_id) = sent_msg[0].clone();

        let mut sphinx_packet = sphinx.clone();

        let mut next_node = route[0];

        for node_id in route {
            println!("Node ID: {}", node_id);
            assert_eq!(next_node, node_id);
            let key_pair = key_pairs.get(&node_id).unwrap();
            let static_secret = &key_pair.1;

            let processed = match sphinx_packet.clone().inner.process(&static_secret) {
                Ok(processed) => processed,
                Err(e) => {
                    log::error!("Failed to process packet: {:?}", e);
                    assert!(false);
                    return;
                }
            };

            match processed {
                ProcessedPacket::ForwardHop(next_packet, next_address, _delay) => {
                    sphinx_packet = Sphinx {
                        message_id: sphinx_packet.message_id.clone(),
                        inner: *next_packet,
                    };
                    next_node = node_id_from_node_address(next_address);
                }
                ProcessedPacket::FinalHop(destination, _, payload) => {
                    let dest = node_id_from_destination_address(destination);
                    assert_eq!(dest, client.get_our_provider().await.unwrap());
                    if dest == node_id {
                        if let Ok(module_message) = serde_yaml::from_str::<NetworkWrapper>(
                            std::str::from_utf8(&payload.recover_plaintext().unwrap()).unwrap(),
                        ) {
                            assert_eq!(module_message.module, MODULE_NAME);

                            let msg =
                                serde_yaml::from_str::<MessageType>(&module_message.msg).unwrap();
                            match msg {
                                MessageType::SubscriptionRequest(id) => {
                                    assert_eq!(id, client.get_our_id().await)
                                }
                                _ => assert!(false),
                            }
                        } else {
                            assert!(false);
                        }
                    } else {
                        assert!(false);
                    }
                }
            }
        }
    }
}
