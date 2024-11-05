use crate::overlay::messages::NetworkWrapper;

use crate::loopix::broker::MODULE_NAME;
use crate::loopix::messages::{LoopixMessage, MessageType};
use crate::loopix::core::LoopixCore;
use crate::loopix::mixnode::MixnodeInterface;
use crate::loopix::sphinx::Sphinx;
use crate::loopix::storage::LoopixStorage;
use crate::loopix::config::CoreConfig;
use flarch::nodeids::NodeID;
use sphinx_packet::header::delays::Delay;
use sphinx_packet::SphinxPacket;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use tokio::time::sleep;

use super::sphinx::node_id_from_node_address;

#[derive(Debug, Clone, PartialEq)]
pub struct Provider {
    storage: LoopixStorage,
    config: CoreConfig,
}

impl MixnodeInterface for Provider {
    async fn process_forward_hop(&self, next_packet: Box<SphinxPacket>, next_node: NodeID, delay: Delay) -> (NodeID, Delay, Option<Sphinx>) {
        if self.get_storage().get_clients().await.contains(&next_node) {
            let sphinx = &Sphinx { inner: *next_packet };
            self.store_client_message(next_node, sphinx.clone()).await;
            (next_node, delay, Some(sphinx.clone()))
        } else {
            Box::pin(MixnodeInterface::process_forward_hop(self, next_packet, next_node, delay)).await // TODO I have no idea what box pin is, compoiler said to do it
        }
    }
}


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

}

impl Provider {
    pub fn new(storage: LoopixStorage, config: CoreConfig) -> Self {
        Self { 
            storage, 
            config 
        }
    }

    pub async fn subscribe_client(&self, client_id: NodeID) {
        self.get_storage().add_client(client_id).await;
    }

    pub async fn get_client_messages(&self, client_id: NodeID) -> Vec<Sphinx> {
        self.get_storage().get_client_messages(client_id).await
    }

    pub async fn store_client_message(&self, client_id: NodeID, message: Sphinx) {
        self.get_storage().add_client_message(client_id, message).await
    }

    async fn create_dummy_message(&self, client_id: NodeID) -> Sphinx {
        // create route
        let route = self.create_route(0, None, None, Some(client_id)).await;

        // create message
        let dummy_msg = serde_json::to_string(&MessageType::Dummy).unwrap();
        let msg = NetworkWrapper{ module: MODULE_NAME.into(), msg: dummy_msg};

        // create sphinx packet
        let (_, sphinx) = self.create_sphinx_packet(client_id, msg, &route);
        sphinx
    }

    async fn create_pull_reply(&self, client_id: NodeID) -> Vec<Sphinx> {
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
            let sphinx = self.create_dummy_message(client_id).await;
            messages_to_send.push(sphinx);
        }

        messages_to_send
    }
}
