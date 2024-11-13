use std::sync::Arc;
use std::time::Duration;

use crate::overlay::messages::NetworkWrapper;

use crate::loopix::broker::MODULE_NAME;
use crate::loopix::config::CoreConfig;
use crate::loopix::core::LoopixCore;
use crate::loopix::messages::MessageType;
use crate::loopix::sphinx::Sphinx;
use crate::loopix::storage::LoopixStorage;
use async_trait::async_trait;
use flarch::nodeids::NodeID;
use sphinx_packet::header::delays::{generate_from_average_duration, Delay};
use sphinx_packet::payload::Payload;
use sphinx_packet::SphinxPacket;

use super::sphinx::node_id_from_node_address;

#[derive(Debug, PartialEq)]
pub struct Provider {
    storage: Arc<LoopixStorage>,
    config: CoreConfig,
}

#[async_trait]
impl LoopixCore for Provider {
    fn get_config(&self) -> &CoreConfig {
        &self.config
    }

    fn get_storage(&self) -> &Arc<LoopixStorage> {
        &self.storage
    }

    async fn get_our_id(&self) -> NodeID {
        self.storage.get_our_id().await
    }

    // THIS IS COPY PASTED FROM MIXNODE, I JUST DON'T KNOW HOW TO DO TRAITS IN RUST
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
        self.storage
            .add_sent_message(route, MessageType::Loop)
            .await;
        (node_id_from_node_address(next_node.address), sphinx)
    }

    // THIS IS COPY PASTED FROM MIXNODE, I JUST DON'T KNOW HOW TO DO TRAITS IN RUST
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
        self.storage
            .add_sent_message(route, MessageType::Drop)
            .await;
        (node_id_from_node_address(next_node.address), sphinx)
    }

    async fn process_final_hop(
        &self,
        destination: NodeID,
        _surb_id: [u8; 16],
        payload: Payload,
    ) -> (
        NodeID,
        Option<NetworkWrapper>,
        Option<Vec<(Delay, Sphinx)>>,
        Option<MessageType>,
    ) {
        if destination != self.get_our_id().await {
            log::info!("Final hop received, but we're not the destination");
            return (destination, None, None, None);
        }

        if let Ok(module_message) = serde_yaml::from_str::<NetworkWrapper>(
            std::str::from_utf8(&payload.recover_plaintext().unwrap()).unwrap(),
        ) {
            if module_message.module == MODULE_NAME {
                if let Ok(message) = serde_yaml::from_str::<MessageType>(&module_message.msg) {
                    match message {
                        MessageType::Payload(_, _) => {
                            log::error!("Provider shouldn't receive payloads!");
                            (destination, None, None, Some(message))
                        }
                        MessageType::PullRequest(client_id) => {
                            let messages = self.create_pull_reply(client_id).await;
                            log::trace!(
                                "Provider received pull request from client: {:?}",
                                client_id
                            );
                            (client_id, None, Some(messages), Some(message))
                        }
                        MessageType::SubscriptionRequest(client_id) => {
                            self.get_storage().add_subscribed_client(client_id).await;
                            log::trace!(
                                "Provider received subscription request from client: {:?}",
                                client_id
                            );
                            (client_id, None, None, Some(message))
                        }
                        MessageType::Drop => {
                            log::trace!("Provider received drop");
                            (destination, None, None, Some(message))
                        }
                        MessageType::Loop => {
                            log::trace!("Provider received loop");
                            (destination, None, None, Some(message))
                        }
                        MessageType::Dummy => {
                            log::error!("Provider shouldn't receive dummy messages!");
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
        next_packet: Box<SphinxPacket>,
        next_node: NodeID,
        delay: Delay,
    ) -> (NodeID, Delay, Option<Sphinx>) {
        if self
            .get_storage()
            .get_subscribed_clients()
            .await
            .contains(&next_node)
        {
            let sphinx = &Sphinx {
                inner: *next_packet,
            };
            self.store_client_message(next_node, delay, sphinx.clone())
                .await;

            (next_node, delay, None)
        } else {
            // THIS IS COPY PASTED FROM MIXNODE, I JUST DON'T KNOW HOW TO DO TRAITS IN RUST
            let sphinx = &Sphinx {
                inner: *next_packet,
            };
            log::debug!(
                "{} --> {}: {:?}",
                self.get_our_id().await,
                next_node,
                sphinx
            );
            (next_node, delay, Some(sphinx.clone()))
        }
    }
}

impl Provider {
    pub fn new(storage: Arc<LoopixStorage>, config: CoreConfig) -> Self {
        Self { storage, config }
    }

    pub async fn async_clone(&self) -> Self {
        let storage_clone = Arc::new(self.storage.async_clone().await);

        Provider {
            storage: storage_clone,
            config: self.config.clone(),
        }
    }

    pub async fn subscribe_client(&self, client_id: NodeID) {
        self.get_storage().add_subscribed_client(client_id).await;
    }

    pub async fn get_client_messages(&self, client_id: NodeID) -> Vec<(Delay, Sphinx)> {
        self.get_storage().get_client_messages(client_id).await
    }

    pub async fn store_client_message(&self, client_id: NodeID, delay: Delay, message: Sphinx) {
        self.get_storage()
            .add_client_message(client_id, delay, message)
            .await
    }

    pub async fn create_dummy_message(&self, client_id: NodeID) -> (Delay, Sphinx) {
        // create route
        let route = self.create_route(0, None, None, Some(client_id)).await;

        // create message
        let dummy_msg = serde_json::to_string(&MessageType::Dummy).unwrap();
        let msg = NetworkWrapper {
            module: MODULE_NAME.into(),
            msg: dummy_msg,
        };

        // create sphinx packet
        let (_, sphinx) = self.create_sphinx_packet(client_id, msg, &route);

        // create delay
        let mean_delay = Duration::from_millis(self.get_config().mean_delay());
        let delay = generate_from_average_duration(1, mean_delay);
        self.storage
            .add_sent_message(route, MessageType::Dummy)
            .await;

        (delay[0], sphinx)
    }

    pub async fn create_pull_reply(&self, client_id: NodeID) -> Vec<(Delay, Sphinx)> {
        // get max send amount and messages
        log::trace!("Creating pull reply for client: {}", client_id);
        let max_retrieve = self.get_config().max_retrieve();
        let messages = self.get_client_messages(client_id).await;

        log::debug!(
            "Pull reply has {} real messages and {} messages will be send",
            messages.len(),
            max_retrieve
        );
        // add messages to send
        let mut messages_to_send = Vec::new();
        for message in messages.iter().take(max_retrieve) {
            messages_to_send.push(message.clone());
        }

        // pad vec if not enough messages
        // for _ in messages_to_send.len()..max_retrieve {
        //     let (delay, sphinx) = self.create_dummy_message(client_id).await;
        //     messages_to_send.push((delay, sphinx));
        // } // TODO uncomment

        messages_to_send
    }
}
