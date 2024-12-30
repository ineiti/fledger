use flarch::nodeids::NodeID;
use std::{sync::Arc, time::{Duration, SystemTime}};
use tokio::sync::{mpsc::{channel, Receiver, Sender}, Semaphore};

use flarch::{
    broker::{Broker, BrokerError, Subsystem, SubsystemHandler},
    platform_async_trait,
};

use crate::{
    loopix::{BANDWIDTH, CLIENT_DELAY}, network::messages::{NetworkIn, NetworkMessage, NetworkOut}, overlay::messages::NetworkWrapper
};

use super::{
    client::Client,
    config::{LoopixConfig, LoopixRole},
    core::*,
    messages::{LoopixIn, LoopixMessage, LoopixMessages, LoopixOut, NodeType},
    mixnode::Mixnode,
    provider::Provider,
    sphinx::Sphinx,
    storage::LoopixStorage,
};

pub const MODULE_NAME: &str = "Loopix";

pub struct LoopixBroker {
    pub broker: Broker<LoopixMessage>,
    pub storage: Arc<LoopixStorage>,
}

impl LoopixBroker {
    pub async fn start(
        network: Broker<NetworkMessage>,
        config: LoopixConfig,
        n_duplicates: u8,
    ) -> Result<LoopixBroker, BrokerError> {
        let mut broker = Broker::new();

        broker
            .link_bi(
                network.clone(),
                Box::new(Self::from_network),
                Box::new(Self::to_network),
            )
            .await?;

        let storage = Arc::new(config.storage_config);

        let node_type = match config.role {
            LoopixRole::Client => {
                NodeType::Client(Client::new(Arc::clone(&storage), config.core_config))
            }
            LoopixRole::Provider => {
                NodeType::Provider(Provider::new(Arc::clone(&storage), config.core_config))
            }
            LoopixRole::Mixnode => {
                NodeType::Mixnode(Mixnode::new(Arc::clone(&storage), config.core_config))
            }
        };

        let (network_sender, network_receiver): (
            Sender<(NodeID, Sphinx, Option<SystemTime>)>,
            Receiver<(NodeID, Sphinx, Option<SystemTime>)>,
        ) = channel(200);

        let (overlay_sender, overlay_receiver): (
            Sender<(NodeID, NetworkWrapper)>,
            Receiver<(NodeID, NetworkWrapper)>,
        ) = channel(200);

        let loopix_messages = LoopixMessages::new(
            node_type.arc_clone(),
            network_sender.clone(),
            overlay_sender.clone(),
            n_duplicates,
        );

        let max_workers = Arc::new(Semaphore::new(8));

        broker
            .add_subsystem(Subsystem::Handler(Box::new(LoopixTranslate {
                loopix_messages: loopix_messages.clone(),
                max_workers: Arc::clone(&max_workers),
            })))
            .await?;

        // Threads for all nodes
        match node_type {
            // Client has a queue to send messages to the network
            // Lambda_payload per minute, it sends either a message from the queue or a drop message
            // It sends loop traffic and drop traffic as configured
            // it also sends subscribe and pull messages continually
            NodeType::Client(_) => {
                // network send thread
                Self::client_payload_thread(
                    loopix_messages.clone(),
                    broker.clone(),
                    network_receiver,
                );

                // drop message thread
                Self::start_drop_message_thread(loopix_messages.clone(), broker.clone());

                // // client subscribe loop
                Self::client_subscribe_thread(loopix_messages.clone(), broker.clone());

                // client pull loop
                Self::client_pull_thread(loopix_messages.clone(), broker.clone());

                // messages that get sent back to this node are distributed from the overlay
                Self::start_overlay_send_thread(broker.clone(), overlay_receiver);
            }
            // Mixnode and provider forwards messages immediately (after the delay has been introduced)
            NodeType::Provider(_) | NodeType::Mixnode(_) => {
                Self::mixnode_forward_thread(broker.clone(), network_receiver);
            }
        }

        // All nodes send loop messages
        // All nodes need to notify other brokers about which nodes are connected
        Self::start_loop_message_thread(loopix_messages.clone(), broker.clone());

        Self::start_infos_connected_thread(broker.clone(), loopix_messages.clone());

        Ok(LoopixBroker {
            broker,
            storage: Arc::clone(&storage),
        })
    }

    fn from_network(msg: NetworkMessage) -> Option<LoopixMessage> {
        if let NetworkMessage::Output(NetworkOut::MessageFromNode(node_id, message)) = msg {
            let sphinx_packet: Result<Sphinx, _> = serde_yaml::from_str(&message);
            if let Ok(sphinx_packet) = sphinx_packet {
                return Some(LoopixIn::SphinxFromNetwork(node_id, sphinx_packet).into());
            }
        }
        None
    }

    fn to_network(msg: LoopixMessage) -> Option<NetworkMessage> {
        if let LoopixMessage::Output(LoopixOut::SphinxToNetwork(node_id, sphinx)) = msg {
            let msg = serde_yaml::to_string(&sphinx).unwrap();
            return Some(NetworkIn::MessageToNode(node_id, msg).into());
        }
        None
    }

    pub fn start_infos_connected_thread(
        mut broker: Broker<LoopixMessage>,
        loopix_messages: LoopixMessages,
    ) {
        tokio::spawn(async move {
            loop {
                // emit node infos connected
                let node_infos = loopix_messages.role.get_connected_nodes().await;
                if let Err(e) =
                    broker.emit_msg(LoopixOut::NodeInfosConnected(node_infos.clone()).into())
                {
                    log::error!("Failed to emit node infos connected message: {:?}", e);
                } else {
                    log::debug!("Nodeinfos connected message emitted: {:?}", node_infos);
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    }

    pub fn start_overlay_send_thread(
        mut broker: Broker<LoopixMessage>,
        mut receiver: Receiver<(NodeID, NetworkWrapper)>,
    ) {
        tokio::spawn(async move {
            loop {
                if let Some((node_id, wrapper)) = receiver.recv().await {
                    if let Err(e) =
                        broker.emit_msg(LoopixOut::OverlayReply(node_id, wrapper.clone()).into())
                    {
                        log::error!("Error emitting overlay message: {e:?}");
                    } else {
                        log::debug!(
                            "Loopix to Overlay from {}, wrapper len: {:?}",
                            node_id,
                            wrapper
                        );
                    }
                }
            }
        });
    }

    pub fn start_loop_message_thread(
        loopix_messages: LoopixMessages,
        mut broker: Broker<LoopixMessage>,
    ) {
        let lambda_loop = match loopix_messages.role {
            NodeType::Client(_) => loopix_messages.role.get_config().lambda_loop(),
            NodeType::Mixnode(_) => loopix_messages.role.get_config().lambda_loop_mix(),
            NodeType::Provider(_) => loopix_messages.role.get_config().lambda_loop_mix(),
        };

        if lambda_loop == 0.0 {
            log::info!("Loop message rate: 0.0, skipping thread");
            return;
        }

        let wait_before_send =
            Duration::from_secs_f64(1.0 / lambda_loop);

        log::debug!("Loop message rate: {:?}", wait_before_send);

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(wait_before_send).await;

                // create loop message
                let (node_id, sphinx) = loopix_messages.role.create_loop_message().await;
                log::debug!(
                    "Sending loop message with id {} to {}",
                    sphinx.message_id,
                    node_id
                );

                // emit loop message
                if let Err(e) = broker.emit_msg(LoopixOut::SphinxToNetwork(node_id, sphinx.clone()).into())
                {
                    log::error!("Failed to emit loop message: {:?}", e);
                } else {
                    log::trace!("Successfully emitted loop message to node {}", node_id);
                    BANDWIDTH.inc_by(sphinx.inner.to_bytes().len() as f64);
                }
            }
        });
    }

    pub fn start_drop_message_thread(
        loopix_messages: LoopixMessages,
        mut broker: Broker<LoopixMessage>,
    ) {
        let lambda_drop = loopix_messages.role.get_config().lambda_drop();
        if lambda_drop == 0.0 {
            log::info!("Drop message rate: 0.0, skipping thread");
            return;
        }

        let wait_before_send =
            Duration::from_secs_f64(1.0 / lambda_drop);

        log::debug!("Drop message rate: {:?}", wait_before_send);

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(wait_before_send).await;

                // create drop message
                let (node_id, sphinx) = loopix_messages.create_drop_message().await;
                log::debug!(
                    "Sending drop message with id {} to {}",
                    sphinx.message_id,
                    node_id
                );

                // emit drop message
                if let Err(e) = broker.emit_msg(LoopixOut::SphinxToNetwork(node_id, sphinx.clone()).into())
                {
                    log::error!("Failed to emit drop message: {:?}", e);
                } else {
                    log::trace!("Successfully emitted drop message to node {}", node_id);
                    BANDWIDTH.inc_by(sphinx.inner.to_bytes().len() as f64);
                }
            }
        });
    }

    pub fn client_subscribe_thread(
        mut loopix_messages: LoopixMessages,
        mut broker: Broker<LoopixMessage>,
    ) {
        let subscribe_request_rate =
            Duration::from_secs_f64(loopix_messages.role.get_config().time_pull());
        // let subscribe_request_rate = Duration::from_secs_f64(1.0);

        log::debug!("Subscribe request rate: {:?}", subscribe_request_rate);

        tokio::spawn(async move {
            loop {
                // subscribe message
                let (node_id, sphinx) = loopix_messages.create_subscribe_message().await;
                log::trace!(
                    "Sending subscribe message with id {} to {}",
                    sphinx.message_id,
                    node_id
                );

                // serialize and send
                // let msg = serde_yaml::to_string(&sphinx).unwrap();
                if let Err(e) = broker.emit_msg(LoopixOut::SphinxToNetwork(node_id, sphinx.clone()).into())
                {
                    log::error!("Failed to emit subscribe message: {:?}", e);
                } else {
                    log::trace!("Successfully emitted subscribe message to node {}", node_id);
                    BANDWIDTH.inc_by(sphinx.inner.to_bytes().len() as f64);
                }

                // wait
                tokio::time::sleep(subscribe_request_rate).await;
            }
        });
    }

    pub fn client_pull_thread(loopix_messages: LoopixMessages, mut broker: Broker<LoopixMessage>) {
        let pull_request_rate =
            Duration::from_secs_f64(loopix_messages.role.get_config().time_pull());

        log::debug!("Pull request rate: {:?}", pull_request_rate);

        tokio::spawn(async move {
            loop {
                // pull message
                let (node_id, sphinx) = loopix_messages.clone().create_pull_message().await;
                if let Some(sphinx) = sphinx.clone() {
                    log::trace!(
                        "Sending pull message with id {} to {}",
                        sphinx.message_id,
                        node_id
                    );
                } else {
                    log::debug!("Why don't we send a pull message?");
                }

                if let Some(sphinx) = sphinx {
                    if let Err(e) =
                        broker.emit_msg(LoopixOut::SphinxToNetwork(node_id, sphinx.clone()).into())
                    {
                        log::error!("Failed to emit pull message: {:?}", e);
                    } else {
                        log::trace!("Successfully emitted pull message to node {}", node_id);
                        BANDWIDTH.inc_by(sphinx.inner.to_bytes().len() as f64);
                    }
                }

                // wait
                tokio::time::sleep(pull_request_rate).await;
            }
        });
    }

    pub fn mixnode_forward_thread(
        mut broker: Broker<LoopixMessage>,
        mut receiver: Receiver<(NodeID, Sphinx, Option<SystemTime>)>,
    ) {
        tokio::spawn(async move {
            loop {
                if let Some((node_id, sphinx, _)) = receiver.recv().await {
                    if let Err(e) = broker
                        .emit_msg(LoopixOut::SphinxToNetwork(node_id, sphinx.clone()).into())
                    {
                        log::error!("Error emitting forward message: {e:?}");
                    } else {
                        log::trace!(
                            "Loopix to Network from {}, message id: {:?}",
                            node_id,
                            sphinx.message_id
                        );
                        BANDWIDTH.inc_by(sphinx.inner.to_bytes().len() as f64);
                    }
                }
            }
        });
    }

    pub fn client_payload_thread(
        loopix_messages: LoopixMessages,
        mut broker: Broker<LoopixMessage>,
        mut receiver: Receiver<(NodeID, Sphinx, Option<SystemTime>)>,
    ) {
        let lambda_payload = loopix_messages.role.get_config().lambda_payload();

        if lambda_payload == 0.0 {
            panic!("Lambda payload cannot be 0.0");
        }

        let wait_before_send = Duration::from_secs_f64(1.0 / lambda_payload);

        tokio::spawn(async move {
            
            let mut sphinx_messages: Vec<(NodeID, Sphinx, Option<SystemTime>)> = Vec::new();

            let our_id = loopix_messages.role.get_our_id().await;

            log::info!(
                "{}: Real message rate is {:?} per second. Wait before send: {:?}",
                our_id,
                lambda_payload,
                wait_before_send
            );

            loop {
                // Receive new messages
                while let Ok((node_id, sphinx, timestamp)) = receiver.try_recv() {
                    log::trace!("{}: Received message for node {}", our_id, node_id);
                    sphinx_messages.push((node_id, sphinx, timestamp));
                }

                if let Some((node_id, sphinx, timestamp)) = sphinx_messages.first() {
                    log::info!(
                        "The queue is of length {:?}, first message is to node {} with id {:?}",
                        sphinx_messages.len(),
                        node_id,
                        sphinx.message_id
                    );
                    if let Err(e) =

                        broker.emit_msg(LoopixOut::SphinxToNetwork(*node_id, sphinx.clone()).into())
                    {
                        log::error!("Error emitting network message: {e:?}");
                    } else {
                        log::trace!(
                            "{} emitted a message network {:?} to node {}",
                            our_id,
                            sphinx.message_id,
                            node_id
                        );

                        if let Some(timestamp) = timestamp {
                            match timestamp.elapsed() {
                                Ok(elapsed) => {
                                    CLIENT_DELAY.observe(elapsed.as_millis() as f64);
                                }
                                Err(e) => {
                                    log::error!("Error: {:?}", e);
                                }
                            }
                        } else {
                            log::error!("No timestamp found for message");
                        }
                        

                        BANDWIDTH.inc_by(sphinx.inner.to_bytes().len() as f64);
                        sphinx_messages.remove(0);
                    }
                } else {
                    let (node_id, sphinx) = loopix_messages.create_drop_message().await;
                    log::trace!(
                        "Sending drop message with id {} to {}",
                        sphinx.message_id,
                        node_id
                    );
                    if let Err(e) =
                        broker.emit_msg(LoopixOut::SphinxToNetwork(node_id, sphinx.clone()).into())
                    {
                        log::error!("Error emitting drop message: {e:?}");
                    }
                    log::trace!("{} emitted a drop message to node {}", our_id, node_id);
                    BANDWIDTH.inc_by(sphinx.inner.to_bytes().len() as f64);
                }

                // Wait for send delay
                tokio::time::sleep(wait_before_send).await;
            }
        });
    }
}

struct LoopixTranslate {
    loopix_messages: LoopixMessages,
    max_workers: Arc<Semaphore>
}

#[platform_async_trait()]
impl SubsystemHandler<LoopixMessage> for LoopixTranslate {
    async fn messages(&mut self, msgs: Vec<LoopixMessage>) -> Vec<LoopixMessage> {
        for msg in msgs {
            if let LoopixMessage::Input(input) = msg {
                let permit = self.max_workers.clone().acquire_owned().await.unwrap();
                let mut loopix_messages = self.loopix_messages.clone();
                tokio::spawn(async move {
                    loopix_messages.process_messages(vec![input]).await;
                    drop(permit);
                }).await.map_err(|e| log::error!("Error processing messages: {:?}", e)).ok();
            }
        }
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loopix::config::{LoopixConfig, LoopixRole};
    use crate::loopix::messages::MessageType;
    use crate::loopix::sphinx::{destination_address_from_node_id, node_address_from_node_id};
    use crate::loopix::storage::LoopixStorage;
    use crate::loopix::testing::LoopixSetup;
    use crate::network::messages::NetworkMessage;
    use flarch::broker::Broker;
    use flarch::start_logging_filter_level;
    use sphinx_packet::header::delays::Delay;
    use sphinx_packet::route::{Destination, Node};
    use sphinx_packet::SphinxPacket;
    use tokio::time::{timeout, Duration};

    async fn setup_network() -> Result<
        (
            Broker<LoopixMessage>,
            Arc<LoopixStorage>,
            Broker<NetworkMessage>,
        ),
        BrokerError,
    > {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

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
            all_nodes.clone(),
        );

        config
            .storage_config
            .set_node_public_keys(node_public_keys)
            .await;

        let network = Broker::<NetworkMessage>::new();

        let loopix_broker = LoopixBroker::start(network.clone(), config, 1).await?;

        Ok((
            loopix_broker.broker,
            Arc::clone(&loopix_broker.storage),
            network,
        ))
    }

    #[tokio::test]
    async fn create_broker() -> Result<(), BrokerError> {
        let path_length = 2;

        let (all_nodes, node_public_keys, loopix_key_pairs, _) =
            LoopixSetup::create_nodes_and_keys(path_length);

        // take first path length from nodeinfos
        let node_id = all_nodes
            .clone()
            .into_iter()
            .take(path_length)
            .next()
            .unwrap()
            .get_id(); // take a random client
        let private_key = &loopix_key_pairs.get(&NodeID::from(node_id)).unwrap().1;
        let public_key = &loopix_key_pairs.get(&NodeID::from(node_id)).unwrap().0;

        let config = LoopixConfig::default_with_path_length(
            LoopixRole::Client,
            node_id,
            path_length,
            private_key.clone(),
            public_key.clone(),
            all_nodes.clone(),
        );

        config
            .storage_config
            .set_node_public_keys(node_public_keys)
            .await;

        let network = Broker::<NetworkMessage>::new();

        let _loopix_broker = LoopixBroker::start(network, config, 1).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_broker_sends_message() -> Result<(), BrokerError> {
        let (loopix_broker, _, network) = setup_network().await?;

        // Send a test message
        let test_node_id = NodeID::from(1);
        let sphinx_packet = Sphinx::default();
        loopix_broker
            .clone()
            .emit_msg(LoopixMessage::Output(LoopixOut::SphinxToNetwork(
                test_node_id,
                sphinx_packet.clone(),
            )))
            .unwrap();

        // Verify the message was processed and sent to the network
        let (mut tap, _) = network.clone().get_tap().await?;
        if let Ok(Some(NetworkMessage::Input(NetworkIn::MessageToNode(node_id, msg)))) =
            timeout(Duration::from_secs(10), tap.recv()).await
        {
            assert_eq!(node_id, test_node_id);
            assert_eq!(msg, serde_yaml::to_string(&sphinx_packet).unwrap());
        } else {
            panic!("Message not processed by LoopixBroker");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_broker_receives_overlay_request() -> Result<(), BrokerError> {
        let (loopix_broker, storage, network) = setup_network().await?;

        let client_to_provider_map = storage.get_client_to_provider_map().await;
        log::debug!("Client to provider map: {:?}", client_to_provider_map);
        // Simulate sending an overlay request to the LoopixBroker
        let test_node_id = NodeID::from(1);
        let network_wrapper = NetworkWrapper {
            module: "loopix".to_string(),
            msg: "test".to_string(),
        };
        loopix_broker
            .clone()
            .emit_msg(LoopixMessage::Input(LoopixIn::OverlayRequest(
                test_node_id,
                network_wrapper.clone(),
            )))
            .unwrap();

        // Verify the message was processed and sent to the network
        let (mut tap, _) = network.clone().get_tap().await?;
        if let Ok(Some(NetworkMessage::Input(NetworkIn::MessageToNode(next_node_id, msg)))) =
            timeout(Duration::from_secs(10), tap.recv()).await
        {
            let our_provider = storage.get_our_provider().await.unwrap();
            assert_eq!(next_node_id, our_provider);

            let _ = serde_yaml::from_str::<Sphinx>(&msg).unwrap();
        } else {
            assert!(false, "Message not processed by LoopixBroker");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_broker_receives_network_message() -> Result<(), BrokerError> {
        let (loopix_broker, storage, _) = setup_network().await?;

        //create sphinx packet
        let drop_msg = serde_yaml::to_string(&MessageType::Drop).unwrap();
        let msg = NetworkWrapper {
            module: MODULE_NAME.into(),
            msg: drop_msg,
        };
        let our_node_id = storage.get_our_id().await;
        let public_key = storage.get_public_key().await;

        let surb_identifier = [0u8; 16];
        let destination = Destination {
            address: destination_address_from_node_id(our_node_id),
            identifier: surb_identifier,
        };
        let route = vec![Node {
            address: node_address_from_node_id(our_node_id),
            pub_key: public_key,
        }];
        let delays = vec![Delay::new_from_nanos(1)];
        let message_vec = serde_yaml::to_string(&msg).unwrap().as_bytes().to_vec();
        let sphinx_packet = SphinxPacket::new(message_vec, &route, &destination, &delays).unwrap();
        let sphinx = Sphinx {
            message_id: uuid::Uuid::new_v4().to_string(),
            inner: sphinx_packet,
        };

        // Simulate sending an network message to the LoopixBroker
        loopix_broker
            .clone()
            .emit_msg(LoopixMessage::Input(LoopixIn::SphinxFromNetwork(
                NodeID::rnd(),
                sphinx,
            )))
            .unwrap();

        Ok(())
    }

    #[tokio::test]
    async fn test_send_queue_threads() -> Result<(), BrokerError> {
        let (_, _, network) = setup_network().await?;

        tokio::time::sleep(Duration::from_secs(10)).await;

        let (mut tap, _) = network.clone().get_tap().await?;
        for _ in 0..3 {
            if let Ok(Some(msg)) = timeout(Duration::from_secs(6), tap.recv()).await {
                println!("{:?}", msg);
            } else {
                panic!("Network should receive dummy and drop messages");
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_subtract_wait_duration_from_message_delays() {
        let mut sphinx_messages = vec![
            (NodeID::from(1), Duration::from_secs(10), Sphinx::default()),
            (NodeID::from(2), Duration::from_secs(20), Sphinx::default()),
            (NodeID::from(3), Duration::from_secs(30), Sphinx::default()),
        ];

        let wait_before_send = Duration::from_secs(5);

        for (_, delay, _) in &mut sphinx_messages {
            *delay = delay.saturating_sub(wait_before_send);
        }

        assert_eq!(sphinx_messages[0].1, Duration::from_secs(5));
        assert_eq!(sphinx_messages[1].1, Duration::from_secs(15));
        assert_eq!(sphinx_messages[2].1, Duration::from_secs(25));
    }

    #[tokio::test]
    async fn test_sort_sphinx_messages_by_delay() {
        let mut sphinx_messages = vec![
            (NodeID::from(1), Duration::from_secs(30), Sphinx::default()),
            (NodeID::from(2), Duration::from_secs(10), Sphinx::default()),
            (NodeID::from(3), Duration::from_secs(20), Sphinx::default()),
        ];

        sphinx_messages.sort_by_key(|&(_, delay, _)| delay);

        assert_eq!(sphinx_messages[0].1, Duration::from_secs(10));
        assert_eq!(sphinx_messages[1].1, Duration::from_secs(20));
        assert_eq!(sphinx_messages[2].1, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_receive_and_update_sphinx_messages_queue_with_receiver() {
        let (sender, mut receiver) = tokio::sync::mpsc::channel::<(NodeID, Delay, Sphinx)>(10);

        sender
            .send((
                NodeID::from(1),
                Delay::new_from_nanos(30),
                Sphinx::default(),
            ))
            .await
            .unwrap();
        sender
            .send((
                NodeID::from(2),
                Delay::new_from_nanos(20),
                Sphinx::default(),
            ))
            .await
            .unwrap();
        sender
            .send((
                NodeID::from(3),
                Delay::new_from_nanos(10),
                Sphinx::default(),
            ))
            .await
            .unwrap();

        let mut sphinx_messages: Vec<(NodeID, Duration, Sphinx)> = Vec::new();

        while let Ok((node_id, delay, sphinx)) = receiver.try_recv() {
            sphinx_messages.push((node_id, delay.to_duration(), sphinx));
        }

        sphinx_messages.sort_by_key(|&(_, delay, _)| delay);

        assert_eq!(sphinx_messages[0].1, Duration::from_nanos(10));
        assert_eq!(sphinx_messages[1].1, Duration::from_nanos(20));
        assert_eq!(sphinx_messages[2].1, Duration::from_nanos(30));

        let wait_before_send = Duration::from_nanos(5);
        for (_, delay, _) in &mut sphinx_messages {
            *delay = delay.saturating_sub(wait_before_send);
        }

        assert_eq!(sphinx_messages[0].1, Duration::from_nanos(5));
        assert_eq!(sphinx_messages[1].1, Duration::from_nanos(15));
        assert_eq!(sphinx_messages[2].1, Duration::from_nanos(25));
    }
}
