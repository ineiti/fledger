use std::time::Duration;

use crate::overlay::messages::NetworkWrapper;

use crate::loopix::broker::MODULE_NAME;
use crate::loopix::config::CoreConfig;
use crate::loopix::core::LoopixCore;
use crate::loopix::messages::MessageType;
use crate::loopix::mixnode::MixnodeInterface;
use crate::loopix::sphinx::Sphinx;
use crate::loopix::storage::LoopixStorage;
use async_trait::async_trait;
use flarch::nodeids::NodeID;
use sphinx_packet::header::delays::{generate_from_average_duration, Delay};
use sphinx_packet::payload::Payload;
use sphinx_packet::SphinxPacket;

#[derive(Debug, Clone, PartialEq)]
pub struct Provider {
    storage: LoopixStorage,
    config: CoreConfig,
}

#[async_trait]
impl LoopixCore for Provider {
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
        MixnodeInterface::create_loop_message(self).await
    }

    async fn create_drop_message(&self) -> (NodeID, Sphinx) {
        MixnodeInterface::create_drop_message(self).await
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
                        MessageType::Payload(_, _) => { log::error!("Provider shouldn't receive payloads!"); (destination, None, None) },
                        MessageType::PullRequest(client_id) => { 
                            let messages = self.create_pull_reply(client_id).await;
                            log::info!("Provider received pull request from client: {:?}", client_id);
                            (destination, None, Some(messages))
                        },
                        MessageType::SubscriptionRequest(client_id) => {
                            self.get_storage().add_client(client_id).await;
                            log::info!("Provider received subscription request from client: {:?}", client_id);
                            (destination, None, None)
                        },
                        MessageType::Drop => { log::info!("Provider received drop"); (destination, None, None) },
                        MessageType::Loop => { log::info!("Provider received loop"); (destination, None, None) },
                        MessageType::Dummy => { log::error!("Provider shouldn't receive dummy messages!"); (destination, None, None) },
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

    async fn process_forward_hop(
        &self,
        next_packet: Box<SphinxPacket>,
        next_node: NodeID,
        delay: Delay,
    ) -> (NodeID, Delay, Option<Sphinx>) {
        if self.get_storage().get_clients().await.contains(&next_node) {
            let sphinx = &Sphinx {
                inner: *next_packet,
            };
            self.store_client_message(next_node, delay, sphinx.clone()).await;
            (next_node, delay, Some(sphinx.clone()))
        } else {
            Box::pin(MixnodeInterface::process_forward_hop(
                self,
                next_packet,
                next_node,
                delay,
            ))
            .await // TODO I have no idea what box pin is, compiler said to do it
        }
    }

}

#[async_trait]
impl MixnodeInterface for Provider {
    async fn process_forward_hop(
        &self,
        next_packet: Box<SphinxPacket>,
        next_node: NodeID,
        delay: Delay,
    ) -> (NodeID, Delay, Option<Sphinx>) {
        LoopixCore::process_forward_hop(self, next_packet, next_node, delay).await
    }

    async fn process_final_hop(
        &self,
        destination: NodeID,
        surb_id: [u8; 16],
        payload: Payload,
    ) -> (NodeID, Option<NetworkWrapper>, Option<Vec<(Delay, Sphinx)>>) {
        LoopixCore::process_final_hop(self, destination, surb_id, payload).await
    }
}

impl Provider {
    pub fn new(storage: LoopixStorage, config: CoreConfig) -> Self {
        Self { storage, config }
    }

    pub async fn subscribe_client(&self, client_id: NodeID) {
        self.get_storage().add_client(client_id).await;
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
        let mean_delay = Duration::from_secs_f64(self.get_config().mean_delay());
        let delay = generate_from_average_duration(1, mean_delay);

        (delay[0], sphinx)
    }

    pub async fn create_pull_reply(&self, client_id: NodeID) -> Vec<(Delay, Sphinx)> {
        // get max send amount and messages
        let max_retrieve = self.get_config().max_retrieve();
        let messages = self.get_client_messages(client_id).await;

        // add messages to send
        let mut messages_to_send = Vec::new();
        for message in messages.iter().take(max_retrieve) {
            messages_to_send.push(message.clone());
        }

        // pad vec if not enough messages
        if messages_to_send.len() < max_retrieve {
            let (delay, sphinx) = self.create_dummy_message(client_id).await;
            messages_to_send.push((delay, sphinx));
        }

        messages_to_send
    }
}
