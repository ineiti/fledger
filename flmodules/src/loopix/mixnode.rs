use std::sync::Arc;
use std::time::SystemTime;

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
use sphinx_packet::packet::*;

use crate::overlay::messages::NetworkWrapper;

#[derive(Debug, PartialEq)]
pub struct Mixnode {
    storage: Arc<LoopixStorage>,
    config: CoreConfig,
}

#[async_trait]
impl LoopixCore for Mixnode {
    fn get_storage(&self) -> &Arc<LoopixStorage> {
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
        message_id: String,
    ) -> (NodeID, Option<Sphinx>) {
        let sphinx = &Sphinx {
            message_id: message_id.clone(),
            inner: *next_packet,
        };
        log::debug!(
            "{} --> {}: {:?}",
            self.get_our_id().await,
            next_node,
            message_id.clone()
        );
        (next_node, Some(sphinx.clone()))
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

        let plaintext = payload.recover_plaintext().unwrap();
        let plaintext_str = std::str::from_utf8(&plaintext).unwrap();

        if let Ok(module_message) = serde_yaml::from_str::<NetworkWrapper>(plaintext_str) {
            if module_message.module == MODULE_NAME {
                if let Ok(message) = serde_yaml::from_str::<MessageType>(&module_message.msg) {
                    match message {
                        MessageType::Payload(_, _) => {
                            log::warn!("Mixnode shouldn't receive payloads!");
                            (destination, None, None, Some(message))
                        }
                        MessageType::PullRequest(_) => {
                            log::warn!("Mixnode shouldn't receive pull requests!");
                            (destination, None, None, Some(message))
                        }
                        MessageType::SubscriptionRequest(_) => {
                            log::warn!("Mixnode shouldn't receive subscription requests!");
                            (destination, None, None, Some(message))
                        }
                        MessageType::Drop => {
                            log::trace!("Mixnode received drop");
                            (destination, None, None, Some(message))
                        }
                        MessageType::Loop => {
                            log::trace!("Mixnode received loop");
                            (destination, None, None, Some(message))
                        }
                        MessageType::Dummy => {
                            log::warn!("Mixnode shouldn't receive dummy messages!");
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

    async fn create_loop_message(&self) -> (NodeID, Sphinx) {
        let providers = self.get_storage().get_providers().await;
        let our_id = self.get_our_id().await;

        // pick random provider
        let random_provider = providers.iter().next().unwrap();

        // create route
        // As a mixnode, the node routes it's loop message through a random provider
        // the destination is the node's own ID
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
        // self.storage
        //     .add_sent_message(route, MessageType::Loop, sphinx.message_id.clone())
        //     .await; // TODO uncomment
        (node_id_from_node_address(next_node.address), sphinx)
    }

}

impl Mixnode {
    pub fn new(storage: Arc<LoopixStorage>, config: CoreConfig) -> Self {
        Self { storage, config }
    }

    pub async fn async_clone(&self) -> Self {
        let storage_clone = Arc::new(self.storage.async_clone().await);

        Mixnode {
            storage: storage_clone,
            config: self.config.clone(),
        }
    }
}
