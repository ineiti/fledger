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
use rand::seq::SliceRandom;
use sphinx_packet::{header::delays::Delay, packet::*};

use crate::overlay::messages::NetworkWrapper;

#[derive(Debug, Clone, PartialEq)]
pub struct Mixnode {
    storage: LoopixStorage,
    config: CoreConfig,
}

#[async_trait]
pub trait MixnodeInterface: LoopixCore {
    async fn process_forward_hop(
        &self,
        next_packet: Box<SphinxPacket>,
        next_address: NodeID,
        delay: Delay,
    ) -> (NodeID, Delay, Option<Sphinx>);

    async fn create_loop_message(&self) -> (NodeID, Sphinx) {
        let providers = self.get_storage().get_providers().await;
        let our_id = self.get_our_id().await;

        // pick random provider
        let random_provider = providers.choose(&mut rand::thread_rng()).unwrap();

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
        let providers = self.get_storage().get_providers().await;

        // pick random provider
        let random_provider = providers.choose(&mut rand::thread_rng()).unwrap();

        // create route
        let route = self
            .create_route(
                self.get_config().path_length(),
                None,
                Some(*random_provider),
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
        let (next_node, sphinx) = self.create_sphinx_packet(*random_provider, msg, &route);
        (node_id_from_node_address(next_node.address), sphinx)
    }
}

impl Mixnode {
    pub fn new(storage: LoopixStorage, config: CoreConfig) -> Self {
        Self { storage, config }
    }
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

    async fn create_loop_message(&self) -> (NodeID, Sphinx) {
        MixnodeInterface::create_loop_message(self).await
    }

    async fn create_drop_message(&self) -> (NodeID, Sphinx) {
        MixnodeInterface::create_drop_message(self).await
    }
}

#[async_trait]
impl MixnodeInterface for Mixnode {
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
}
