use flarch::nodeids::NodeID;
use sphinx_packet::header::delays::Delay;
use std::time::Duration;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use flarch::{
    broker::{Broker, BrokerError, Subsystem, SubsystemHandler},
    platform_async_trait,
};

use crate::{
    network::messages::{NetworkIn, NetworkMessage, NetworkOut},
    overlay::messages::NetworkWrapper,
};

use super::{
    client::Client,
    config::{LoopixConfig, LoopixRole},
    core::*,
    messages::{LoopixIn, LoopixMessage, LoopixMessages, LoopixOut, NodeType},
    mixnode::Mixnode,
    provider::Provider,
    sphinx::Sphinx,
};

pub const MODULE_NAME: &str = "Loopix";

pub struct LoopixBroker {
    pub broker: Broker<LoopixMessage>,
}

impl LoopixBroker {
    pub async fn start(
        network: Broker<NetworkMessage>,
        config: LoopixConfig,
    ) -> Result<Broker<LoopixMessage>, BrokerError> {
        let mut broker = Broker::new();

        broker
            .link_bi(
                network.clone(),
                Box::new(Self::from_network),
                Box::new(Self::to_network),
            )
            .await?;

        let node_type = match config.role {
            LoopixRole::Client => {
                NodeType::Client(Client::new(config.storage_config, config.core_config))
            }
            LoopixRole::Provider => {
                NodeType::Provider(Provider::new(config.storage_config, config.core_config))
            }
            LoopixRole::Mixnode => {
                NodeType::Mixnode(Mixnode::new(config.storage_config, config.core_config))
            }
        };

        let (network_sender, network_receiver): (
            Sender<(NodeID, Delay, Sphinx)>,
            Receiver<(NodeID, Delay, Sphinx)>,
        ) = channel(200);
        let (overlay_sender, overlay_receiver): (Sender<(NodeID, NetworkWrapper)>, Receiver<(NodeID, NetworkWrapper)>) =
            channel(200);

        let loopix_messages = LoopixMessages::new(
            node_type.arc_clone(),
            network_sender.clone(),
            overlay_sender.clone(),
        );

        broker
            .add_subsystem(Subsystem::Handler(Box::new(LoopixTranslate {
                network: network.clone(),
                loopix_messages: loopix_messages.clone(),
            })))
            .await?;

        // Threads for all nodes
        Self::start_network_send_thread(loopix_messages.clone(), broker.clone(), network_receiver);
        
        Self::start_overlay_send_thread(broker.clone(), overlay_receiver);
        
        Self::start_loop_message_thread(loopix_messages.clone());

        Self::start_drop_message_thread(loopix_messages.clone());

        // Client has two extra threads
        match node_type {
            NodeType::Client(_) => {
                // client subscribe loop
                Self::client_subscribe_loop(loopix_messages.clone(), network.clone());

                // client pull loop
                Self::client_pull_loop(loopix_messages.clone(), network.clone());
            }
            _ => {}
        }

        // emit node infos connectedrole
        let node_infos = loopix_messages.role.get_connected_nodes().await;
        broker.emit_msg(LoopixOut::NodeInfosConnected(node_infos).into()).unwrap();

        Ok(broker)
    }

    fn from_network(msg: NetworkMessage) -> Option<LoopixMessage> {
        if let NetworkMessage::Output(NetworkOut::MessageFromNode(_node_id, message)) = msg {
            // TODO: probably node_id should be used somewhere.
            let sphinx_packet: Sphinx = serde_yaml::from_str(&message).unwrap();
            return Some(LoopixIn::SphinxFromNetwork(sphinx_packet).into());
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

    pub fn start_loop_message_thread(loopix_messages: LoopixMessages) {
        let wait_before_send = Duration::from_secs_f64(60.0 / loopix_messages.role.get_config().lambda_loop());

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(wait_before_send).await;
                loopix_messages.send_loop_message().await;
            }
        });
    }

    pub fn start_drop_message_thread(loopix_messages: LoopixMessages) {
        let wait_before_send = Duration::from_secs_f64(60.0 / loopix_messages.role.get_config().lambda_drop());

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(wait_before_send).await;
                loopix_messages.send_drop_message().await;
            }
        });
    }

    pub fn start_overlay_send_thread(mut broker: Broker<LoopixMessage>, mut receiver: Receiver<(NodeID, NetworkWrapper)>) {
        tokio::spawn(async move {
            loop {
                if let Some((node_id, wrapper)) = receiver.recv().await {
                    if let Err(e) = broker.emit_msg(LoopixOut::OverlayReply(node_id, wrapper).into()) {
                        log::error!("Error emitting overlay message: {e:?}");
                    }
                }
            }
        });
    }

    pub fn start_network_send_thread(loopix_messages: LoopixMessages, mut broker: Broker<LoopixMessage>, mut receiver: Receiver<(NodeID, Delay, Sphinx)>) {
        tokio::spawn(async move {
            let mut sphinx_messages: Vec<(NodeID, Duration, Sphinx)> = Vec::new();
            let wait_before_send = match loopix_messages.role {
                NodeType::Client(_) => {
                    log::trace!("this is wait before send {}", 60.0 / loopix_messages.role.get_config().lambda_payload());
                    Duration::from_secs_f64(60.0 / loopix_messages.role.get_config().lambda_payload())
                }
                NodeType::Provider(_) | NodeType::Mixnode(_) => {
                    log::trace!("this is wait before send {}", 60.0 / loopix_messages.role.get_config().lambda_loop_mix());
                    Duration::from_secs_f64(60.0 / loopix_messages.role.get_config().lambda_loop_mix())
                }
            };

            loop {
                log::trace!("Started network send thread with {:?} rate!", wait_before_send);

                // Wait for send delay
                tokio::time::sleep(wait_before_send).await;

                // Subtract the wait duration from all message delays
                for (_, delay, _) in &mut sphinx_messages {
                    *delay = delay.saturating_sub(wait_before_send);
                }

                // Receive new messages
                if let Some((node_id, delay, sphinx)) = receiver.recv().await {
                    sphinx_messages.push((node_id, delay.to_duration(), sphinx));
                }

                log::trace!("length of queue {}", sphinx_messages.len());

                // Sort messages by remaining delay
                sphinx_messages.sort_by_key(|&(_, delay, _)| delay); // TODO technically this is not the protocol

                // Emit messages with 0 or less delay
                if let Some((node_id, delay, sphinx_packet)) = sphinx_messages.first() {
                    if *delay <= Duration::ZERO {
                        if let Err(e) =
                            broker.emit_msg(LoopixOut::SphinxToNetwork(*node_id, sphinx_packet.clone()).into())
                        {
                            log::error!("Error emitting networkmessage: {e:?}");
                        } else {
                            sphinx_messages.remove(0);
                        }
                    } else {
                        let (node_id, sphinx) = loopix_messages.create_drop_message().await;
                        if let Err(e) =
                            broker.emit_msg(LoopixOut::SphinxToNetwork(node_id, sphinx.clone()).into())
                        {
                            log::error!("Error emitting drop message: {e:?}");
                        }
                    }
                }
            }
        });
    }

    pub fn client_subscribe_loop(
        mut loopix_messages: LoopixMessages,
        mut network: Broker<NetworkMessage>,
    ) {
        let pull_request_rate =
            Duration::from_secs_f64(loopix_messages.role.get_config().time_pull());

        tokio::spawn(async move {
            loop {
                // subscribe message
                let (node_id, sphinx) = loopix_messages.create_subscribe_message().await;

                // serialize and send
                let msg = serde_yaml::to_string(&sphinx).unwrap();
                network
                    .emit_msg(NetworkIn::MessageToNode(node_id, msg).into())
                    .unwrap();

                // wait
                tokio::time::sleep(pull_request_rate).await;
            }
        });
    }

    pub fn client_pull_loop(loopix_messages: LoopixMessages, mut network: Broker<NetworkMessage>) {
        let pull_request_rate =
            Duration::from_secs_f64(loopix_messages.role.get_config().time_pull());

        tokio::spawn(async move {
            loop {
                // pull message
                let (node_id, sphinx) = loopix_messages.create_pull_message().await;

                if let Some(sphinx) = sphinx {
                    // serialize and send
                    let msg = serde_yaml::to_string(&sphinx).unwrap();
                    network
                        .emit_msg(NetworkIn::MessageToNode(node_id, msg).into())
                        .unwrap();
                }

                // wait
                tokio::time::sleep(pull_request_rate).await;
            }
        });
    }
}

struct LoopixTranslate {
    network: Broker<NetworkMessage>,
    loopix_messages: LoopixMessages,
}

#[platform_async_trait()]
impl SubsystemHandler<LoopixMessage> for LoopixTranslate {
    async fn messages(&mut self, msgs: Vec<LoopixMessage>) -> Vec<LoopixMessage> {

        for msg in msgs {
            match msg {
                LoopixMessage::Input(input) => {
                    self.loopix_messages.process_messages(vec![input]).await;
                }
                LoopixMessage::Output(output) => {
                    match output {
                        LoopixOut::SphinxToNetwork(node_id, sphinx) => {
                            self.network.emit_msg(NetworkIn::MessageToNode(node_id, serde_yaml::to_string(&sphinx).unwrap()).into()).unwrap();
                        }
                        _ => {}
                    }
                }
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
    use crate::network::messages::NetworkMessage;
    use flarch::broker::Broker;
    use flarch::start_logging_filter_level;
    use sphinx_packet::route::{Destination, Node};
    use sphinx_packet::SphinxPacket;
    use tokio::time::{timeout, Duration};
    use crate::loopix::testing::LoopixSetup;

    async fn setup_network() -> Result<(Broker<LoopixMessage>, LoopixStorage, Broker<NetworkMessage>), BrokerError> {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let path_length = 2;
        
        let (all_nodes, node_public_keys, loopix_key_pairs, _) = LoopixSetup::create_nodes_and_keys(path_length);

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
        let loopix_storage = config.storage_config.arc_clone();
        
        let loopix_broker = LoopixBroker::start(network.clone(), config).await?;

        Ok((loopix_broker, loopix_storage, network))
    }

    #[tokio::test]
    async fn create_broker() -> Result<(), BrokerError> {
        let path_length = 2;

        let (all_nodes, node_public_keys, loopix_key_pairs, _) = LoopixSetup::create_nodes_and_keys(path_length);

        // take first path length from nodeinfos
        let node_id = all_nodes.clone().into_iter().take(path_length).next().unwrap().get_id(); // take a random client
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

        let _loopix_broker = LoopixBroker::start(network, config).await?;
        
        Ok(())
    }

    #[tokio::test]
    async fn test_broker_sends_message() -> Result<(), BrokerError> {
        let (loopix_broker, _, network) = setup_network().await?;

        // Send a test message
        let test_node_id = NodeID::from(1);
        let sphinx_packet = Sphinx::default();
        loopix_broker.clone().emit_msg(LoopixMessage::Output(LoopixOut::SphinxToNetwork(test_node_id, sphinx_packet.clone()))).unwrap();

        // Verify the message was processed and sent to the network
        let (mut tap, _) = network.clone().get_tap().await?;
        if let Ok(Some(NetworkMessage::Input(NetworkIn::MessageToNode(node_id, msg)))) = timeout(Duration::from_secs(10), tap.recv()).await {
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
        println!("Client to provider map: {:?}", client_to_provider_map);
        // Simulate sending an overlay request to the LoopixBroker
        let test_node_id = NodeID::from(1);
        let network_wrapper = NetworkWrapper{module: "loopix".to_string(), msg: "test".to_string()};
        loopix_broker.clone().emit_msg(LoopixMessage::Input(LoopixIn::OverlayRequest(test_node_id, network_wrapper.clone()))).unwrap();

        // Verify the message was processed and sent to the network
        let (mut tap, _) = network.clone().get_tap().await?;
        if let Ok(Some(NetworkMessage::Input(NetworkIn::MessageToNode(next_node_id, msg)))) = timeout(Duration::from_secs(10), tap.recv()).await {
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
        let destination = Destination {address: destination_address_from_node_id(our_node_id), identifier: surb_identifier};
        let route = vec![Node {address: node_address_from_node_id(our_node_id), pub_key: public_key}];
        let delays = vec![Delay::new_from_nanos(1)];
        let message_vec = serde_yaml::to_string(&msg).unwrap().as_bytes().to_vec();
        let sphinx_packet = SphinxPacket::new(message_vec, &route, &destination, &delays).unwrap();
        
        // Simulate sending an network message to the LoopixBroker
        loopix_broker.clone().emit_msg(LoopixMessage::Input(LoopixIn::SphinxFromNetwork(Sphinx{inner: sphinx_packet}))).unwrap();

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

}
