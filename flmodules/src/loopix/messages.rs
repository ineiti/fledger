use std::fmt;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use async_trait::async_trait;
use sphinx_packet::header::delays::Delay;

use tokio::sync::mpsc::Sender;

use flarch::nodeids::{NodeID, NodeIDs};
use serde::{Deserialize, Serialize};
use sphinx_packet::{packet::*, payload::*};

use super::config::CoreConfig;
use super::storage::LoopixStorage;
use super::{client::Client, core::*, mixnode::Mixnode, provider::Provider, sphinx::*};
use super::{DECRYPTION_LATENCY, MIXNODE_DELAY};
use crate::loopix::{INCOMING_MESSAGES, PROVIDER_DELAY};
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
    // packet in sphinx format, from other nodes (NodeID is source node)
    SphinxFromNetwork(NodeID, Sphinx),
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub enum MessageType {
    Payload(NodeID, NetworkWrapper), // NodeID: Source
    Drop,
    Loop,
    Dummy,
    PullRequest(NodeID),
    SubscriptionRequest(NodeID),
}

impl fmt::Display for MessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageType::Payload(node_id, payload) => {
                write!(f, "Payload({}, {:?})", node_id, payload)
            }
            MessageType::Drop => write!(f, "Drop"),
            MessageType::Loop => write!(f, "Loop"),
            MessageType::Dummy => write!(f, "Dummy"),
            MessageType::PullRequest(node_id) => write!(f, "PullRequest({})", node_id),
            MessageType::SubscriptionRequest(node_id) => {
                write!(f, "SubscriptionRequest({})", node_id)
            }
        }
    }
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
    pub network_sender: Sender<(NodeID, Sphinx, Option<SystemTime>)>,
    pub overlay_sender: Sender<(NodeID, NetworkWrapper)>,
    pub n_duplicates: u8,
}

impl Clone for LoopixMessages {
    fn clone(&self) -> Self {
        Self {
            role: self.role.arc_clone(),
            network_sender: self.network_sender.clone(),
            overlay_sender: self.overlay_sender.clone(),
            n_duplicates: self.n_duplicates,
        }
    }
}

impl LoopixMessages {
    pub fn new(
        node_type: NodeType,
        network_sender: Sender<(NodeID, Sphinx, Option<SystemTime>)>,
        overlay_sender: Sender<(NodeID, NetworkWrapper)>,
        n_duplicates: u8,
    ) -> Self {
        Self {
            role: node_type,
            network_sender: network_sender,
            overlay_sender: overlay_sender,
            n_duplicates: n_duplicates,
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
                log::info!(
                    "{}: OverlayRequest with node_id {} and message {:?}",
                    self.role.get_our_id().await,
                    node_id,
                    message
                );
                self.process_overlay_message(node_id, message).await;
            }
            LoopixIn::SphinxFromNetwork(node_id, sphinx) => {
                log::trace!(
                    "{}: SphinxFromNetwork from node_id {} and sphinx {:?}",
                    self.role.get_our_id().await,
                    node_id,
                    sphinx.message_id
                );
                INCOMING_MESSAGES.inc();
                self.process_sphinx_packet(node_id, sphinx).await;
            }
        }
    }

    pub async fn create_drop_message(&self) -> (NodeID, Sphinx) {
        self.role.create_drop_message().await
    }

    pub async fn create_subscribe_message(&mut self) -> (NodeID, Sphinx) {
        self.role.create_subscribe_message().await
    }

    pub async fn create_pull_message(self) -> (NodeID, Option<Sphinx>) {
        self.role.create_pull_message().await
    }

    async fn process_overlay_message(&self, node_id: NodeID, message: NetworkWrapper) {
        log::debug!("Processing overlay message to send {} duplicates", self.n_duplicates);
        for _ in 0..self.n_duplicates {
            let (next_node, sphinx) = self.role.process_overlay_message(node_id, message.clone()).await;

            log::trace!(
                "{}: Overlay message to {:?}",
                self.role.get_our_id().await,
                next_node,
            );

            self.network_sender
                .send((next_node, sphinx, Some(SystemTime::now())))
                .await
                .expect("while sending overlay message to network");
        }
    }

    async fn process_sphinx_packet(&self, node_id: NodeID, sphinx_packet: Sphinx) {
        let start_time = Instant::now();
        let processed = self.role.process_sphinx_packet(sphinx_packet.clone()).await;
        if let Some(processed) = processed {
            match processed {
                ProcessedPacket::ForwardHop(next_packet, next_address, delay) => {
                    log::debug!("ForwardHop from node_id {} and sphinx {:?}", node_id, sphinx_packet.message_id);

                    // process the message
                    let next_node_id = node_id_from_node_address(next_address);
                    let (next_node_id, sphinx) = self
                        .role
                        .process_forward_hop(next_packet, next_node_id, sphinx_packet.message_id)
                        .await;

                    // if there is any packets to forward, send it after the delay
                    if let Some(sphinx) = sphinx {
                        // self.role
                        //     .get_storage()
                        //     .add_forwarded_message((
                        //         node_id,
                        //         next_node_id,
                        //         sphinx.message_id.clone(),
                        //     ))
                        //     .await; // from node_id to next_node_id

                        // send the message after the delay passes
                        self.role
                            .send_after_sphinx_packet(
                                self.network_sender.clone(),
                                delay,
                                next_node_id,
                                sphinx,
                            )
                            .await;
                    } else {
                        log::debug!("No message to forward to {}", next_node_id);
                    }

                    // observe the decryption latency
                    let end_time = start_time.elapsed().as_millis() as f64;
                    DECRYPTION_LATENCY.observe(end_time);
                }
                ProcessedPacket::FinalHop(destination, surb_id, payload) => {
                    // process the message
                    let dest = node_id_from_destination_address(destination);
                    let (source, msg, messages, _message_type) =
                        self.role.process_final_hop(dest, surb_id, payload).await;

                    // if let Some(message_type) = _message_type {
                    //     self.role
                    //         .get_storage()
                    //         .add_received_message((
                    //             source,
                    //             dest,
                    //             message_type,
                    //             sphinx_packet.message_id.clone(),
                    //         ))
                    //         .await;
                    // }

                    // case 1: message is a payload
                    if let Some(msg) = msg {
                        log::debug!(
                            "Final hop was a payload message with id: {} -> {}: {:?}",
                            source,
                            dest,
                            sphinx_packet.message_id
                        );
                        self.overlay_sender
                            .send((source, msg))
                            .await
                            .expect("while sending message");
                    }

                    // case 2: message is a pull request
                    if let Some((node_id, messages)) = messages {
                        let message_details: Vec<_> = messages
                            .iter()
                            .map(|(sphinx, _)| &sphinx.message_id)
                            .collect();
                        log::trace!(
                            "Final hop was a pull request: {} -> {} with message IDs: {:?}",
                            node_id,
                            messages.len(),
                            message_details
                        );

                        for (message, timestamp) in messages {

                            // observe time since provider saved the message to it's storage
                            if let Some(timestamp) = timestamp {
                                let end_time = match timestamp.elapsed() {
                                    Ok(elapsed) => elapsed.as_millis() as f64,

                                    Err(e) => {
                                        log::error!("Error: {:?}", e);
                                        continue;
                                    }
                                };
                                PROVIDER_DELAY.observe(end_time);
                            } else {
                                log::trace!("No timestamp found for message");
                            }

                            // send to the network (immediate)
                            self.network_sender
                                .send((node_id, message, None))
                                .await
                                .expect("while sending message");
                        }

                        // observe the decryption latency
                        let end_time = start_time.elapsed().as_millis() as f64;
                        DECRYPTION_LATENCY.observe(end_time);
                    }
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

    fn get_storage(&self) -> &Arc<LoopixStorage> {
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

    async fn create_loop_message(&self) -> (NodeID, Sphinx) {
        match self {
            NodeType::Client(client) => client.create_loop_message().await,
            NodeType::Mixnode(mixnode) => mixnode.create_loop_message().await,
            NodeType::Provider(provider) => provider.create_loop_message().await,
        }
    }

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
    ) {
        match self {
            NodeType::Client(client) => {
                client
                    .process_final_hop(destination, surb_id, payload)
                    .await
            }
            NodeType::Mixnode(mixnode) => {
                mixnode
                    .process_final_hop(destination, surb_id, payload)
                    .await
            }
            NodeType::Provider(provider) => {
                provider
                    .process_final_hop(destination, surb_id, payload)
                    .await
            }
        }
    }

    async fn process_forward_hop(
        &self,
        next_packet: Box<SphinxPacket>,
        next_node: NodeID,
        message_id: String,
    ) -> (NodeID, Option<Sphinx>) {
        match self {
            NodeType::Client(client) => {
                client
                    .process_forward_hop(next_packet, next_node, message_id)
                    .await
            }
            NodeType::Mixnode(mixnode) => {
                mixnode
                    .process_forward_hop(next_packet, next_node, message_id)
                    .await
            }
            NodeType::Provider(provider) => {
                provider
                    .process_forward_hop(next_packet, next_node, message_id)
                    .await
            }
        }
    }
}

impl NodeType {
    pub fn arc_clone(&self) -> Self {
        match self {
            NodeType::Client(client) => {
                let storage = client.get_storage();
                let loopix_storage = Arc::clone(&storage);
                NodeType::Client(Client::new(loopix_storage, client.get_config().clone()))
            }
            NodeType::Mixnode(mixnode) => {
                let storage = mixnode.get_storage();
                let loopix_storage = Arc::clone(&storage);
                NodeType::Mixnode(Mixnode::new(loopix_storage, mixnode.get_config().clone()))
            }
            NodeType::Provider(provider) => {
                let storage = provider.get_storage();
                let loopix_storage = Arc::clone(&storage);
                NodeType::Provider(Provider::new(loopix_storage, provider.get_config().clone()))
            }
        }
    }

    async fn send_after_sphinx_packet(
        &self,
        network_sender: Sender<(NodeID, Sphinx, Option<SystemTime>)>,
        delay: Delay,
        node_id: NodeID,
        sphinx: Sphinx,
    ) {
        match self {
            NodeType::Mixnode(_) | NodeType::Provider(_) => {
                tokio::spawn(async move {
                    let start_time = Instant::now();
                    tokio::time::sleep(delay.to_duration()).await;
                    let end_time = start_time.elapsed().as_millis() as f64;
                    MIXNODE_DELAY.observe(end_time);
                    network_sender
                        .send((node_id, sphinx, None))
                        .await
                        .expect("while sending sphinx packet to network");
                });
            }
            NodeType::Client(_) => panic!("Client should not wait a delay before sending"),
        }
    }

    async fn create_drop_message(&self) -> (NodeID, Sphinx) {
        match self {
            NodeType::Client(client) => client.create_drop_message().await,
            NodeType::Mixnode(_) => panic!("Mixnode does not implement create_drop_message"),
            NodeType::Provider(_) => panic!("Provider does not implement create_drop_message"),
        }
    }

    async fn create_subscribe_message(&mut self) -> (NodeID, Sphinx) {
        match self {
            NodeType::Client(client) => client.create_subscribe_message().await,
            NodeType::Mixnode(_) => panic!("Mixnode does not implement create_subscribe_message"),
            NodeType::Provider(_) => panic!("Provider does not implement create_subscribe_message"),
        }
    }

    async fn create_pull_message(self) -> (NodeID, Option<Sphinx>) {
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
