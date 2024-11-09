use crate::overlay::messages::NetworkWrapper;
use async_trait::async_trait;
use flarch::nodeids::NodeID;
use sphinx_packet::header::delays::generate_from_average_duration;
use sphinx_packet::route::{Destination, Node, NodeAddressBytes};
use sphinx_packet::{ProcessedPacket, SphinxPacket};
use std::time::Duration;

use crate::loopix::config::CoreConfig;
use crate::loopix::sphinx::{destination_address_from_node_id, Sphinx};
use crate::loopix::storage::LoopixStorage;

use rand::prelude::SliceRandom;

#[async_trait]
pub trait LoopixCore {
    fn get_config(&self) -> &CoreConfig;
    fn get_storage(&self) -> &LoopixStorage;
    async fn get_our_id(&self) -> NodeID;

    async fn process_sphinx_packet(&self, sphinx_packet: Sphinx) -> ProcessedPacket {
        let secret_key = self.get_storage().get_private_key().await;
        sphinx_packet.inner.process(&secret_key).unwrap()
    }

    async fn create_loop_message(&self) -> (NodeID, Sphinx);
    async fn create_drop_message(&self) -> (NodeID, Sphinx);

    fn create_sphinx_packet(
        &self,
        dest: NodeID,
        msg: NetworkWrapper,
        route: &[Node],
    ) -> (Node, Sphinx) {
        // delays
        let mean_delay = Duration::from_secs_f64(self.get_config().mean_delay());
        let delays = generate_from_average_duration(route.len(), mean_delay);

        // destination
        let destination_address = destination_address_from_node_id(dest);
        let random_identifier = rand::random::<[u8; 16]>();
        let destination = Destination::new(destination_address, random_identifier);

        // message conversion
        let msg_bytes = serde_yaml::to_vec(&msg).unwrap();

        let sphinx_packet = SphinxPacket::new(msg_bytes, route, &destination, &delays).unwrap();
        (
            route[0].clone(),
            Sphinx {
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
            let start_key = node_public_keys.get(&start_provider).unwrap();
            let start_node = Node::new(
                NodeAddressBytes::from_bytes(start_provider.to_bytes()),
                *start_key,
            );
            route.push(start_node);
        }
        // add mixnode route
        let mixnodes = self.get_storage().get_mixes().await;
        for i in 0..path_length {
            let mixnode = mixnodes[i as usize]
                .choose(&mut rand::thread_rng())
                .unwrap();
            let key = node_public_keys.get(mixnode).unwrap();
            let node = Node::new(NodeAddressBytes::from_bytes(mixnode.to_bytes()), *key);
            route.push(node);
        }

        // add dst provider
        if let Some(dest_provider) = dest_provider {
            let dest_key = node_public_keys.get(&dest_provider).unwrap();
            let dest_node = Node::new(
                NodeAddressBytes::from_bytes(dest_provider.to_bytes()),
                *dest_key,
            );
            route.push(dest_node);
        }

        // add receiver
        if let Some(receiver) = receiver {
            let receiver_key = node_public_keys.get(&receiver).unwrap();
            let receiver_node = Node::new(
                NodeAddressBytes::from_bytes(receiver.to_bytes()),
                *receiver_key,
            );
            route.push(receiver_node);
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
    use sphinx_packet::{packet::*, route::*};
    use std::collections::HashMap;
    use x25519_dalek::{PublicKey, StaticSecret};

    use super::*;

    struct MockLoopixCore {
        config: CoreConfig,
        storage: LoopixStorage,
    }

    #[async_trait]
    impl LoopixCore for MockLoopixCore {
        fn get_config(&self) -> &CoreConfig {
            &self.config
        }

        fn get_storage(&self) -> &LoopixStorage {
            &self.storage
        }

        async fn get_our_id(&self) -> NodeID {
            self.storage.get_our_id().await
        }

        async fn create_loop_message(&self) -> (NodeID, Sphinx) {
            todo!()
        }

        async fn create_drop_message(&self) -> (NodeID, Sphinx) {
            todo!()
        }
    }

    async fn setup() -> (
        MockLoopixCore,
        HashMap<NodeID, (PublicKey, StaticSecret)>,
        usize,
    ) {
        let path_length = 2;
        let mut node_public_keys = HashMap::new();
        let mut node_key_pairs = HashMap::new();

        for mix in 0..path_length * path_length + path_length + path_length {
            let node_id = NodeID::from(mix as u32);
            let (public_key, private_key) = LoopixStorage::generate_key_pair();
            node_public_keys.insert(node_id, public_key);
            node_key_pairs.insert(node_id, (public_key, private_key));
        }

        let node_id = 5;
        let private_key = &node_key_pairs.get(&NodeID::from(node_id)).unwrap().1;
        let public_key = &node_key_pairs.get(&NodeID::from(node_id)).unwrap().0;

        let config = LoopixConfig::default_with_path_length(
            LoopixRole::Mixnode,
            node_id,
            path_length,
            private_key.clone(),
            public_key.clone(),
        );

        let core = MockLoopixCore {
            config: config.core_config,
            storage: config.storage_config,
        };

        core.get_storage()
            .set_node_public_keys(node_public_keys)
            .await;

        (core, node_key_pairs, path_length)
    }

    #[tokio::test]
    async fn test_create_route() {
        let (core, node_key_pairs, _path_length) = setup().await;

        let our_id = core.get_our_id().await;

        let route = core.create_route(0, None, None, Some(our_id)).await;

        assert_eq!(route.len(), 1);

        assert_eq!(
            route[0].address,
            NodeAddressBytes::from_bytes(our_id.to_bytes())
        );

        let public_key = node_key_pairs.get(&our_id).unwrap().0;
        assert_eq!(public_key, route[0].pub_key);
    }

    #[tokio::test]
    async fn test_sphinx_packet_simple() {
        let (core, node_key_pairs, _) = setup().await;

        let our_id = core.get_our_id().await;
        let storage = core.get_storage();
        let private_key = storage.get_private_key().await;

        assert_eq!(
            private_key.as_bytes(),
            node_key_pairs.get(&our_id).unwrap().1.as_bytes()
        );

        // just create a packet with one layer of encryption
        let route = core.create_route(0, None, None, Some(our_id)).await;

        let msg = NetworkWrapper {
            module: MODULE_NAME.into(),
            msg: serde_yaml::to_string(&MessageType::Dummy).unwrap(),
        };

        let (_, sphinx) = core.create_sphinx_packet(our_id, msg, &route);

        let processed = sphinx.inner.process(&private_key).unwrap();

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
        let (core, node_key_pairs, _) = setup().await;

        let our_id = core.get_our_id().await;
        let storage = core.get_storage();
        let private_key = storage.get_private_key().await;

        assert_eq!(
            private_key.as_bytes(),
            node_key_pairs.get(&our_id).unwrap().1.as_bytes()
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
            let node_secret_key = &node_key_pairs.get(&node_id).unwrap().1;
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
