use crate::loopix::broker::MODULE_NAME;

use crate::loopix::{
    config::CoreConfig,
    core::LoopixCore,
    messages::MessageType,
    sphinx::{node_id_from_node_address, Sphinx},
    storage::LoopixStorage,
};
use async_trait::async_trait;
use flarch::nodeids::NodeID;
use sphinx_packet::payload::Payload;
use sphinx_packet::{header::delays::Delay, packet::*};

use crate::overlay::messages::NetworkWrapper;

#[derive(Debug, Clone, PartialEq)]
pub struct Mixnode {
    storage: LoopixStorage,
    config: CoreConfig,
}

#[async_trait]
impl LoopixCore for Mixnode {
    fn get_storage(&self) -> &LoopixStorage {
        &self.storage
    }

    fn get_config(&self) -> &CoreConfig {
        &self.config
    }

    async fn get_our_id(&self) -> NodeID {
        self.storage.get_our_id().await
    }

    async fn process_forward_hop(
        &self,
        next_packet: Box<SphinxPacket>,
        next_node: NodeID,
        delay: Delay,
    ) -> (NodeID, Delay, Option<Sphinx>) {
        let sphinx = &Sphinx {
            inner: *next_packet,
        };
        (next_node, delay, Some(sphinx.clone()))
    }

    async fn process_final_hop(
        &self,
        destination: NodeID,
        _surb_id: [u8; 16],
        payload: Payload,
    ) -> (NodeID, Option<NetworkWrapper>, Option<Vec<(Delay, Sphinx)>>) {
        
        if destination != self.get_our_id().await {
            log::info!("Final hop received, but we're not the destination");
            return (destination, None, None);
        }

        let plaintext = payload.recover_plaintext().unwrap();
        let plaintext_str = std::str::from_utf8(&plaintext).unwrap();

        if let Ok(module_message) = serde_yaml::from_str::<NetworkWrapper>(plaintext_str) {
            if module_message.module == MODULE_NAME {
                if let Ok(message) = serde_yaml::from_str::<MessageType>(&module_message.msg) {
                    match message {
                        MessageType::Payload(_, _) => { log::error!("Mixnode shouldn't receive payloads!"); (destination, None, None) },
                        MessageType::PullRequest(_) => { log::error!("Mixnode shouldn't receive pull requests!"); (destination, None, None) },
                        MessageType::SubscriptionRequest(_) => { log::error!("Mixnode shouldn't receive subscription requests!"); (destination, None, None) },
                        MessageType::Drop => { log::info!("Mixnode received drop"); (destination, None, None) },
                        MessageType::Loop => { log::info!("Mixnode received loop"); (destination, None, None) },
                        MessageType::Dummy => { log::error!("Mixnode shouldn't receive dummy messages!"); (destination, None, None) },
                    }
                } else {
                    log::error!("Received message in wrong format");
                    (destination, None, None)
                }
            } else {
                log::error!("Received message from module that is not Loopix: {:?}", module_message.module);
                (destination, None, None)
            }
        } else {
            log::error!("Could not recover plaintext");
            (destination, None, None)
        }
    }

    async fn create_loop_message(&self) -> (NodeID, Sphinx) {
        let providers = self.get_storage().get_providers().await;
        let our_id = self.get_our_id().await;

        // pick random provider
        let random_provider = providers.iter().next().unwrap();

        // create route
        let route = self
            .create_route(
                self.get_config().path_length(),
                None,
                Some(*random_provider),
                Some(our_id),
            )
            .await;

        // create the networkmessage
        let loop_msg = serde_json::to_string(&MessageType::Loop).unwrap();
        let msg = NetworkWrapper {
            module: MODULE_NAME.into(),
            msg: loop_msg,
        };

        // create sphinx packet
        let (next_node, sphinx) = self.create_sphinx_packet(our_id, msg, &route);
        (node_id_from_node_address(next_node.address), sphinx)
    }

    async fn create_drop_message(&self) -> (NodeID, Sphinx) {
        let random_provider = self.get_storage().get_random_provider().await;

        // create route
        let route = self
            .create_route(
                self.get_config().path_length(),
                None,
                Some(random_provider),
                None,
            )
            .await;

        // create the networkmessage
        let drop_msg = serde_json::to_string(&MessageType::Drop).unwrap();
        let msg = NetworkWrapper {
            module: MODULE_NAME.into(),
            msg: drop_msg,
        };

        // create sphinx packet
        let (next_node, sphinx) = self.create_sphinx_packet(random_provider, msg, &route);
        (node_id_from_node_address(next_node.address), sphinx)
    }
}

impl Mixnode {
    pub fn new(storage: LoopixStorage, config: CoreConfig) -> Self {
        Self { storage, config }
    }
}
