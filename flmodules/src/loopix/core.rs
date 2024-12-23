use crate::overlay::messages::NetworkWrapper;
use async_trait::async_trait;
use flarch::nodeids::NodeID;
use sphinx_packet::header::delays::generate_from_average_duration;
use sphinx_packet::payload::Payload;
use sphinx_packet::route::{Destination, Node, NodeAddressBytes};
use sphinx_packet::{ProcessedPacket, SphinxPacket, SphinxPacketBuilder};
use std::time::{Duration, SystemTime};

use crate::loopix::config::CoreConfig;
use crate::loopix::sphinx::{destination_address_from_node_id, Sphinx};
use crate::loopix::storage::LoopixStorage;
use std::sync::Arc;

use rand::prelude::SliceRandom;

use super::messages::MessageType;

const MAX_PAYLOAD_SIZE: usize = 10240;

#[async_trait]
pub trait LoopixCore {
    fn get_config(&self) -> &CoreConfig;
    fn get_storage(&self) -> &Arc<LoopixStorage>;
    async fn get_our_id(&self) -> NodeID;

    async fn process_sphinx_packet(&self, sphinx_packet: Sphinx) -> Option<ProcessedPacket> {
        let secret_key = self.get_storage().get_private_key().await;
        match sphinx_packet.inner.process(&secret_key) {
            Ok(processed_packet) => Some(processed_packet),
            Err(e) => {
                log::error!(
                    "Failed to process sphinx packet {}: {:?}",
                    sphinx_packet.message_id,
                    e
                );
                None
            }
        }
    }

    async fn create_loop_message(&self) -> (NodeID, Sphinx);

    async fn process_final_hop(
        &self,
        destination: NodeID,
        surb_id: [u8; 16],
        payload: Payload,
    ) -> (
        NodeID,
        Option<NetworkWrapper>,
        Option<(NodeID, Vec<(Sphinx, Option<SystemTime>)>)>,
        Option<MessageType>,
    );
    async fn process_forward_hop(
        &self,
        next_packet: Box<SphinxPacket>,
        next_address: NodeID,
        message_id: String,
    ) -> (NodeID, Option<Sphinx>);

    fn create_sphinx_packet(
        &self,
        dest: NodeID,
        msg: NetworkWrapper,
        route: &[Node],
    ) -> (Node, Sphinx) {
        // delays
        let mean_delay = Duration::from_millis(self.get_config().mean_delay());
        let delays = generate_from_average_duration(route.len(), mean_delay);

        // destination
        let destination_address = destination_address_from_node_id(dest);
        let random_identifier = rand::random::<[u8; 16]>();
        let destination = Destination::new(destination_address, random_identifier);

        // message conversion
        let msg_bytes = serde_yaml::to_vec(&msg).unwrap();

        let builder = SphinxPacketBuilder::new().with_payload_size(MAX_PAYLOAD_SIZE);
        let sphinx_packet = builder.build_packet(msg_bytes, route, &destination, &delays).unwrap();
        (
            route[0].clone(),
            Sphinx {
                message_id: uuid::Uuid::new_v4().to_string(),
                inner: sphinx_packet,
            },
        )
    }

    async fn create_route(
        &self,
        path_length: usize,
        start_provider: Option<NodeID>,
        dest_provider: Option<NodeID>,
        receiver: Option<NodeID>,
    ) -> Vec<Node> {
        let mut route = Vec::new();

        let node_public_keys = self.get_storage().get_node_public_keys().await;

        // add client provider
        if let Some(start_provider) = start_provider {
            if let Some(start_key) = node_public_keys.get(&start_provider) {
                let start_node = Node::new(
                    NodeAddressBytes::from_bytes(start_provider.to_bytes()),
                    *start_key,
                );
                route.push(start_node);
            } else {
                log::error!("Start provider key not found for {:?}", start_provider);
                log::info!("List of node public keys: {:?}", node_public_keys);
            }
        }
        // add mixnode route
        let mixnodes = self.get_storage().get_mixes().await;
        for i in 0..path_length {
            if let Some(mixnode) = mixnodes[i as usize].choose(&mut rand::thread_rng()) {
                let key = node_public_keys.get(mixnode).unwrap();
                let node = Node::new(NodeAddressBytes::from_bytes(mixnode.to_bytes()), *key);
                route.push(node);
            } else {
                log::error!("Failed to choose a mixnode at index {}", i);
                log::info!("List of mixnodes: {:?}", mixnodes);
            }
        }

        // add dst provider
        if let Some(dest_provider) = dest_provider {
            if let Some(dest_key) = node_public_keys.get(&dest_provider) {
                let dest_node = Node::new(
                    NodeAddressBytes::from_bytes(dest_provider.to_bytes()),
                    *dest_key,  
                );
                route.push(dest_node);
            } else {
                log::error!("Destination provider key not found for {:?}", dest_provider);
                log::info!("List of node public keys: {:?}", node_public_keys);
            }
        }

        // add receiver
        if let Some(receiver) = receiver {
            if let Some(receiver_key) = node_public_keys.get(&receiver) {
                let receiver_node = Node::new(
                    NodeAddressBytes::from_bytes(receiver.to_bytes()),
                *receiver_key,
            );
                route.push(receiver_node);
            } else {
                log::error!("Receiver key not found for {:?}", receiver);
                log::info!("List of node public keys: {:?}", node_public_keys);
            }
        }

        route
    }
}

#[cfg(test)]
mod tests {
    use crate::loopix::broker::MODULE_NAME;
    use crate::loopix::config::LoopixConfig;
    use crate::loopix::config::LoopixRole;
    use crate::loopix::messages::MessageType;
    use crate::loopix::sphinx::node_id_from_destination_address;
    use crate::loopix::sphinx::node_id_from_node_address;
    use crate::loopix::testing::LoopixSetup;
    use sphinx_packet::payload::Payload;
    use sphinx_packet::{packet::*, route::*};
    use std::collections::HashMap;
    use x25519_dalek::{PublicKey, StaticSecret};

    use super::*;

    struct MockLoopixCore {
        config: CoreConfig,
        storage: Arc<LoopixStorage>,
    }

    #[async_trait]
    impl LoopixCore for MockLoopixCore {
        fn get_config(&self) -> &CoreConfig {
            &self.config
        }

        fn get_storage(&self) -> &Arc<LoopixStorage> {
            &self.storage
        }

        async fn get_our_id(&self) -> NodeID {
            self.storage.get_our_id().await
        }

        async fn create_loop_message(&self) -> (NodeID, Sphinx) {
            todo!()
        }

        async fn process_final_hop(
            &self,
            _destination: NodeID,
            _surb_id: [u8; 16],
            _payload: Payload,
        ) -> (
            NodeID,
            Option<NetworkWrapper>,
            Option<(NodeID, Vec<(Sphinx, Option<SystemTime>)>)>,
            Option<MessageType>,
        ) {
            todo!()
        }

        async fn process_forward_hop(
            &self,
            _next_packet: Box<SphinxPacket>,
            _next_address: NodeID,
            _message_id: String,
        ) -> (NodeID, Option<Sphinx>) {
            todo!()
        }
    }

    async fn setup() -> (
        MockLoopixCore,
        HashMap<NodeID, (PublicKey, StaticSecret)>,
        usize,
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
            all_nodes,
        );

        let core = MockLoopixCore {
            config: config.core_config,
            storage: Arc::new(config.storage_config),
        };

        core.get_storage()
            .set_node_public_keys(node_public_keys)
            .await;

        (core, loopix_key_pairs, path_length)
    }

    #[tokio::test]
    async fn test_create_route() {
        let (core, loopix_key_pairs, _path_length) = setup().await;

        let our_id = core.get_our_id().await;

        let route = core.create_route(0, None, None, Some(our_id)).await;

        assert_eq!(route.len(), 1);

        assert_eq!(
            route[0].address,
            NodeAddressBytes::from_bytes(our_id.to_bytes())
        );

        let public_key = loopix_key_pairs.get(&our_id).unwrap().0;
        assert_eq!(public_key, route[0].pub_key);
    }

    #[tokio::test]
    async fn test_sphinx_packet_simple() {
        let (core, loopix_key_pairs, _) = setup().await;

        let our_id = core.get_our_id().await;
        let storage = core.get_storage();
        let private_key = storage.get_private_key().await;

        assert_eq!(
            private_key.as_bytes(),
            loopix_key_pairs.get(&our_id).unwrap().1.as_bytes()
        );

        // just create a packet with one layer of encryption
        let route = core.create_route(0, None, None, Some(our_id)).await;

        let msg = NetworkWrapper {
            module: MODULE_NAME.into(),
            msg: serde_yaml::to_string(&MessageType::Dummy).unwrap(),
        };

        let (_, sphinx) = core.create_sphinx_packet(our_id, msg, &route);

        let processed = match sphinx.inner.process(&private_key) {
            Ok(processed) => processed,
            Err(e) => {
                assert!(false, "Failed to process sphinx packet: {:?}", e);
                return;
            }
        };

        match processed {
            ProcessedPacket::ForwardHop(_, _, _) => {
                assert!(false);
            }
            ProcessedPacket::FinalHop(destination, _, payload) => {
                // Check if the final destination matches our ID
                let dest = node_id_from_destination_address(destination);
                if dest == our_id {
                    // Recover the network wrapper
                    if let Ok(module_message) = serde_yaml::from_str::<NetworkWrapper>(
                        std::str::from_utf8(&payload.recover_plaintext().unwrap()).unwrap(),
                    ) {
                        // check module name
                        assert_eq!(module_message.module, MODULE_NAME);

                        // check actual message
                        match serde_yaml::from_str::<MessageType>(&module_message.msg) {
                            Ok(MessageType::Dummy) => assert!(true),
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

    #[tokio::test]
    async fn test_sphinx_packet_3_layers() {
        let (core, loopix_key_pairs, _) = setup().await;

        let our_id = core.get_our_id().await;
        let storage = core.get_storage();
        let private_key = storage.get_private_key().await;

        assert_eq!(
            private_key.as_bytes(),
            loopix_key_pairs.get(&our_id).unwrap().1.as_bytes()
        );

        // just create a packet with one layer of encryption
        let route = core
            .create_route(core.get_config().path_length(), None, None, Some(our_id))
            .await;

        // create packet and message
        let msg = NetworkWrapper {
            module: MODULE_NAME.into(),
            msg: serde_yaml::to_string(&MessageType::Dummy).unwrap(),
        };
        let (_, sphinx) = core.create_sphinx_packet(our_id, msg, &route);

        let mut packet_bytes = sphinx.inner.to_bytes();
        for (index, node) in route.iter().enumerate() {
            let node_id = node_id_from_node_address(node.address);
            let node_secret_key = &loopix_key_pairs.get(&node_id).unwrap().1;
            let packet = SphinxPacket::from_bytes(&packet_bytes).unwrap();
            let processed = packet.process(node_secret_key).unwrap();

            match processed {
                ProcessedPacket::ForwardHop(new_packet, next_node, _) => {
                    assert!(index < route.len() - 1);
                    assert!(next_node == route[index + 1].address);
                    packet_bytes = new_packet.to_bytes();
                }
                ProcessedPacket::FinalHop(destination, _, payload) => {
                    if index != route.len() - 1 {
                        assert!(false);
                    }
                    // Check if the final destination matches our ID
                    let dest = node_id_from_destination_address(destination);
                    if dest == our_id {
                        // Recover the network wrapper
                        if let Ok(module_message) = serde_yaml::from_str::<NetworkWrapper>(
                            std::str::from_utf8(&payload.recover_plaintext().unwrap()).unwrap(),
                        ) {
                            // check module name
                            assert_eq!(module_message.module, MODULE_NAME);

                            // check actual message
                            match serde_yaml::from_str::<MessageType>(&module_message.msg) {
                                Ok(MessageType::Dummy) => assert!(true),
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
