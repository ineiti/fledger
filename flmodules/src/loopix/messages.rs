use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use sphinx_packet::header::delays::generate_from_average_duration;

use tokio::sync::mpsc::Sender;

use flarch::nodeids::{NodeID, NodeIDs};
use serde::{Deserialize, Serialize};
use sphinx_packet::{header::delays::Delay, packet::*, payload::*};

use super::config::CoreConfig;
use super::storage::LoopixStorage;
use super::{
    client::Client,
    core::*,
    mixnode::Mixnode,
    provider::Provider,
    sphinx::*,
};
use crate::nodeconfig::NodeInfo;
use crate::overlay::messages::NetworkWrapper;

#[derive(Clone, Debug)]
pub enum LoopixMessage {
    Input(LoopixIn),
    Output(LoopixOut),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoopixIn {
    // message from overlay: needs to be put in a sphinx packet
    OverlayRequest(NodeID, NetworkWrapper),
    // packet in sphinx format, from other nodes
    SphinxFromNetwork(Sphinx),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    Payload(NodeID, NetworkWrapper), // NodeID: Source
    Drop,
    Loop,
    Dummy,
    PullRequest(NodeID),
    SubscriptionRequest(NodeID),
}

#[derive(Debug, Clone)]
pub enum LoopixOut {
    // Sphinx packet Loopix: network will forward it to the next node
    SphinxToNetwork(NodeID, Sphinx),
    // Unencrypted module message to overlay
    OverlayReply(NodeID, NetworkWrapper),

    NodeInfosConnected(Vec<NodeInfo>),

    NodeIDsConnected(NodeIDs),

    NodeInfoAvailable(Vec<NodeInfo>),
}

#[derive(Debug)]
pub struct LoopixMessages {
    pub role: NodeType,
    pub network_sender: Sender<(NodeID, Delay, Sphinx)>,
    pub overlay_sender: Sender<(NodeID, NetworkWrapper)>,
}

impl Clone for LoopixMessages {
    fn clone(&self) -> Self {
        Self {
            role: self.role.arc_clone(),
            network_sender: self.network_sender.clone(),
            overlay_sender: self.overlay_sender.clone(),
        }
    }
}

impl LoopixMessages {
    pub fn new(
        node_type: NodeType,
        network_sender: Sender<(NodeID, Delay, Sphinx)>,    
        overlay_sender: Sender<(NodeID, NetworkWrapper) >, 
    ) -> Self {
        Self {
            role: node_type,
            network_sender: network_sender,
            overlay_sender: overlay_sender,
        }
    }

    pub async fn process_messages(&mut self, msgs: Vec<LoopixIn>) {
        for msg in msgs {
            self.process_message(msg).await;
        }
    }

    async fn process_message(&self, msg: LoopixIn) {
        match msg {
            LoopixIn::OverlayRequest(node_id, message) => {
                log::trace!("OverlayRequest with node_id {} and message {:?}", node_id, message);
                self.process_overlay_message(node_id, message).await
            }
            LoopixIn::SphinxFromNetwork(sphinx) => self.process_sphinx_packet(sphinx).await,
        }
    }

    pub async fn create_drop_message(&self) -> (NodeID, Sphinx) {
        self.role.create_drop_message().await
    }

    pub async fn send_drop_message(&self) {
        let (node_id, sphinx) = self.create_drop_message().await;
        let mean_delay = Duration::from_secs_f64(self.role.get_config().mean_delay());
        let delay = generate_from_average_duration(1, mean_delay);
        self.network_sender
            .send((node_id, delay[0], sphinx))
            .await
            .expect("while sending message");
    }

    pub async fn send_loop_message(&self) {
        let (node_id, sphinx) = self.role.create_loop_message().await;
        let mean_delay = Duration::from_secs_f64(self.role.get_config().mean_delay());
        let delay = generate_from_average_duration(1, mean_delay);
        self.network_sender
            .send((node_id, delay[0], sphinx))
            .await
            .expect("while sending message");
    }

    pub async fn create_subscribe_message(&mut self) -> (NodeID, Sphinx) {
        self.role.create_subscribe_message().await
    }

    pub async fn create_pull_message(&self) -> (NodeID, Option<Sphinx>) {
        self.role.create_pull_message().await
    }

    async fn process_overlay_message(&self, node_id: NodeID, message: NetworkWrapper) {
        let (next_node, sphinx) = self.role.process_overlay_message(node_id, message).await;
        let mean_delay = Duration::from_secs_f64(self.role.get_config().mean_delay());
        let delay = generate_from_average_duration(1, mean_delay);
        self.network_sender
            .send((next_node, delay[0], sphinx))
            .await
            .expect("while sending message");
    }

    async fn process_sphinx_packet(&self, sphinx_packet: Sphinx) {
        let processed = self.role.process_sphinx_packet(sphinx_packet).await;
        match processed {
            ProcessedPacket::ForwardHop(next_packet, next_address, delay) => {
                let next_node_id = node_id_from_node_address(next_address);
                let (next_node_id, next_delay, sphinx) = self
                    .role
                    .process_forward_hop(next_packet, next_node_id, delay)
                    .await;
                if let Some(sphinx) = sphinx {
                    self.network_sender
                        .send((next_node_id, next_delay, sphinx))
                        .await
                        .expect("while sending message");
                }
            }
            ProcessedPacket::FinalHop(destination, surb_id, payload) => {
                // Check if the final destination matches our ID
                let dest = node_id_from_destination_address(destination);
                if dest == self.role.get_our_id().await {
                    let (source, msg, messages) = self.role.process_final_hop(dest, surb_id, payload).await;
                    if let Some(msg) = msg {
                        self.overlay_sender
                            .send((source, msg))
                            .await
                            .expect("while sending message");
                    }
                    if let Some(messages) = messages {
                        for (delay, sphinx) in messages {
                                self.network_sender
                                .send((source, delay, sphinx))
                                .await
                                .expect("while sending message");
                        }
                    }
                } else {
                    log::warn!("Received a FinalHop packet not intended for this node");
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum NodeType {
    Client(Client),
    Mixnode(Mixnode),
    Provider(Provider),
}

#[async_trait]
impl LoopixCore for NodeType {
    fn get_config(&self) -> &CoreConfig {
        match self {
            NodeType::Client(client) => client.get_config(),
            NodeType::Mixnode(mixnode) => mixnode.get_config(),
            NodeType::Provider(provider) => provider.get_config(),
        }
    }

    fn get_storage(&self) -> &LoopixStorage {
        match self {
            NodeType::Client(client) => client.get_storage(),
            NodeType::Mixnode(mixnode) => mixnode.get_storage(),
            NodeType::Provider(provider) => provider.get_storage(),
        }
    }

    async fn get_our_id(&self) -> NodeID {
        match self {
            NodeType::Client(client) => client.get_our_id().await,
            NodeType::Mixnode(mixnode) => mixnode.get_our_id().await,
            NodeType::Provider(provider) => provider.get_our_id().await,
        }
    }

    async fn create_drop_message(&self) -> (NodeID, Sphinx) {
        match self {
            NodeType::Client(client) => client.create_drop_message().await,
            NodeType::Mixnode(mixnode) => LoopixCore::create_drop_message(mixnode).await,
            NodeType::Provider(provider) => LoopixCore::create_drop_message(provider).await,
        }
    }

    async fn create_loop_message(&self) -> (NodeID, Sphinx) {
        match self {
            NodeType::Client(client) => client.create_loop_message().await,
            NodeType::Mixnode(mixnode) => LoopixCore::create_loop_message(mixnode).await,
            NodeType::Provider(provider) => LoopixCore::create_loop_message(provider).await,
        }
    }

    async fn process_final_hop(&self, destination: NodeID, surb_id: [u8; 16], payload: Payload) -> (NodeID, Option<NetworkWrapper>, Option<Vec<(Delay, Sphinx)>>) {
        match self {
            NodeType::Client(client) => client.process_final_hop(destination, surb_id, payload).await,
            NodeType::Mixnode(mixnode) =>  mixnode.process_final_hop(destination, surb_id, payload).await,
            NodeType::Provider(provider) =>  provider.process_final_hop(destination, surb_id, payload).await,
        }
    }

    async fn process_forward_hop(
        &self,
        next_packet: Box<SphinxPacket>,
        next_node: NodeID,
        delay: Delay,
    ) -> (NodeID, Delay, Option<Sphinx>) {
        match self {
            NodeType::Client(_) => panic!("Client does not implement process_forward_hop"),
            NodeType::Mixnode(mixnode) => {
                mixnode.process_forward_hop(next_packet, next_node, delay).await
            }
            NodeType::Provider(provider) => {
                provider.process_forward_hop(next_packet, next_node, delay).await
            }
        }
    }

}

impl NodeType {
    /// Clones the arc of the storage and creates a new NodeType
    pub fn arc_clone(&self) -> Self {
        match self {
            NodeType::Client(client) => {
                let storage = client.get_storage();
                let network_storage = Arc::clone(&storage.network_storage);
                let client_storage = Arc::clone(&storage.client_storage);
                let provider_storage = Arc::clone(&storage.provider_storage);
                let loopix_storage = LoopixStorage {
                    network_storage,
                    client_storage,
                    provider_storage,
                };
                NodeType::Client(Client::new(loopix_storage, client.get_config().clone()))
            }
            NodeType::Mixnode(mixnode) => {
                let storage = mixnode.get_storage();
                let network_storage = Arc::clone(&storage.network_storage);
                let client_storage = Arc::clone(&storage.client_storage);
                let provider_storage = Arc::clone(&storage.provider_storage);
                let loopix_storage = LoopixStorage {
                    network_storage,
                    client_storage,
                    provider_storage,
                };
                NodeType::Mixnode(Mixnode::new(loopix_storage, mixnode.get_config().clone()))
            }
            NodeType::Provider(provider) => {
                let storage = provider.get_storage();
                let network_storage = Arc::clone(&storage.network_storage);
                let client_storage = Arc::clone(&storage.client_storage);
                let provider_storage = Arc::clone(&storage.provider_storage);
                let loopix_storage = LoopixStorage {
                    network_storage,
                    client_storage,
                    provider_storage,
                };
                NodeType::Provider(Provider::new(loopix_storage, provider.get_config().clone()))
            }
        }
    }

    async fn create_subscribe_message(&self) -> (NodeID, Sphinx) {
        match self {
            NodeType::Client(client) => client.create_subscribe_message().await,
            NodeType::Mixnode(_) => panic!("Mixnode does not implement create_subscribe_message"),
            NodeType::Provider(_) => panic!("Provider does not implement create_subscribe_message"),
        }
    }

    async fn create_pull_message(&self) -> (NodeID, Option<Sphinx>) {
        match self {
            NodeType::Client(client) => client.create_pull_message().await,
            NodeType::Mixnode(_) => panic!("Mixnode does not implement create_pull_message"),
            NodeType::Provider(_) => panic!("Provider does not implement create_pull_message"),
        }
    }

    pub async fn get_connected_nodes(&self) -> Vec<NodeInfo> {
        match self {
            NodeType::Client(client) => client.get_storage().get_clients_in_network().await,
            NodeType::Mixnode(_) => Vec::new(),
            NodeType::Provider(_) => Vec::new(),
        }
    }

    async fn process_overlay_message(
        &self,
        node_id: NodeID,
        message: NetworkWrapper,
    ) -> (NodeID, Sphinx) {
        match self {
            NodeType::Client(client) => client.process_overlay_message(node_id, message).await,
            NodeType::Mixnode(_) => panic!("Mixnode does not implement process_overlay_message"),
            NodeType::Provider(_) => panic!("Provider does not implement process_overlay_message"),
        }
    }
}

impl From<LoopixIn> for LoopixMessage {
    fn from(msg: LoopixIn) -> Self {
        LoopixMessage::Input(msg)
    }
}

impl From<LoopixOut> for LoopixMessage {
    fn from(msg: LoopixOut) -> Self {
        LoopixMessage::Output(msg)
    }
}
