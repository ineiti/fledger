use flarch::broker::{Broker, BrokerError};

use crate::loopix::messages::{LoopixIn, LoopixMessage, LoopixOut};

use super::super::messages::{OverlayIn, OverlayMessage, OverlayOut};

/**
 * This uses the Loopix-module to offer an anonymous and privacy-preserving
 * communication through the nodes.
 */
pub struct OverlayLoopix {
    pub broker: Broker<OverlayMessage>,
}

impl OverlayLoopix {
    pub async fn start(
        loopix: Broker<LoopixMessage>,
    ) -> Result<Broker<OverlayMessage>, BrokerError> {
        let mut broker = Broker::new();

        broker
            .link_bi(
                loopix.clone(),
                Box::new(Self::from_loopix),
                Box::new(Self::to_loopix),
            )
            .await?;

        Ok(broker)
    }

    fn from_loopix(msg: LoopixMessage) -> Option<OverlayMessage> {
        if let LoopixMessage::Output(out) = msg {
            let ret = match out {
                LoopixOut::NodeIDsConnected(node_ids) => OverlayOut::NodeIDsConnected(node_ids),
                LoopixOut::NodeInfosConnected(infos) => OverlayOut::NodeInfosConnected(infos),
                LoopixOut::NodeInfoAvailable(availables) => {
                    OverlayOut::NodeInfoAvailable(availables)
                }
                LoopixOut::OverlayReply(node_id, module_msg) => {
                    OverlayOut::NetworkWrapperFromNetwork(node_id, module_msg)
                }
                _ => return None,
            };
            return Some(ret.into());
        }
        None
    }

    fn to_loopix(msg: OverlayMessage) -> Option<LoopixMessage> {
        if let OverlayMessage::Input(input) = msg {
            match input {
                OverlayIn::NetworkWrapperToNetwork(node_id, wrapper) => {
                    return Some(LoopixIn::OverlayRequest(node_id, wrapper).into());
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;
    use std::time::Duration;

    use flarch::{nodeids::NodeID, start_logging_filter_level};
    use tokio::time::timeout;

    use crate::loopix::broker::LoopixBroker;
    use crate::loopix::config::{LoopixConfig, LoopixRole};
    use crate::loopix::storage::LoopixStorage;
    use crate::network::messages::{NetworkIn, NetworkMessage};
    use crate::nodeconfig::NodeInfo;
    use crate::overlay::messages::NetworkWrapper;
    use super::*;
    use crate::loopix::testing::LoopixSetup;

    async fn setup_network() -> Result<(Broker<LoopixMessage>, Arc<LoopixStorage>, Broker<NetworkMessage>), BrokerError> {
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
        
        let loopix_broker = LoopixBroker::start(network.clone(), config).await?;

        Ok((loopix_broker.broker, Arc::clone(&loopix_broker.storage), network))
    }

    fn _check_msgs(msgs: Vec<OverlayMessage>, available: &[NodeInfo], connected: &[NodeInfo]) {
        assert_eq!(3, msgs.len());
        assert_eq!(
            OverlayMessage::Output(OverlayOut::NodeInfoAvailable(available.to_vec())),
            msgs[0]
        );
        assert_eq!(
            OverlayMessage::Output(OverlayOut::NodeIDsConnected(
                connected
                    .iter()
                    .map(|info| info.get_id())
                    .collect::<Vec<NodeID>>()
                    .into()
            )),
            msgs[1]
        );
        assert_eq!(
            OverlayMessage::Output(OverlayOut::NodeInfosConnected(connected.to_vec())),
            msgs[2]
        );
    }

    #[tokio::test]
    async fn test_overlayloopix_sends_message_to_loopixbroker() -> Result<(), BrokerError> {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        // Setup LoopixBroker
        let (loopix_broker, storage, network) = setup_network().await?;

        // Setup OverlayLoopix
        let mut overlay_broker = OverlayLoopix::start(loopix_broker.clone()).await?;

        // Create a test message
        let test_node_id = storage.get_clients_in_network().await[0].get_id();
        let network_wrapper = NetworkWrapper {
            module: "test".to_string(),
            msg: "test".to_string(),
        };

        // Send the message from OverlayLoopix to LoopixBroker
        overlay_broker
            .emit_msg(OverlayMessage::Input(OverlayIn::NetworkWrapperToNetwork(
                test_node_id,
                network_wrapper.clone(),
            )))
            .unwrap();

        // Verify the message was processed by LoopixBroker
        let (mut tap_loopix, _) = loopix_broker.clone().get_tap().await?;
        if let Ok(Some(LoopixMessage::Input(LoopixIn::OverlayRequest(node_id, msg)))) =
            timeout(Duration::from_secs(10), tap_loopix.recv()).await
        {
            log::info!("Received message from overlay: {:?} to node: {:?}", msg, node_id);
            assert_eq!(msg, network_wrapper)   
        }

        // Verify the message was processed and sent to the network
        let (mut tap, _) = network.clone().get_tap().await?;
        if let Ok(Some(NetworkMessage::Input(NetworkIn::MessageToNode(node_id, _msg)))) =
            timeout(Duration::from_secs(10), tap.recv()).await
        {
            let provider = storage.get_our_provider().await.unwrap();
            // log::info!("Received message from network: {:?} to node: {:?}", msg, node_id);
            // log::info!("Provider: {:?}", provider);
            assert_eq!(node_id, provider);
        } else {
            panic!("Message not processed by LoopixBroker");
        }

        let received_messages = storage.get_received_messages().await;
        log::info!("Received messages: {:?}", received_messages.len());
        for (source, dest, message_type) in received_messages {
            log::info!("Received message from {} to {} with type {:?}", source, dest, message_type);
        }
    
        let forwarded_messages = storage.get_forwarded_messages().await;
        log::info!("Forwarded messages: {:?}", forwarded_messages.len());
        for (source, dest) in forwarded_messages {
            log::info!("Forwarded message from {} to {}", source, dest);
        }

        let sent_messages = storage.get_sent_messages().await;
        log::info!("Sent messages: {:?}", sent_messages.len());

        println!("{:<60} {:<20}", "Route", "Message Type");
        println!("{:-<80}", "");

        for (route, message_type) in sent_messages {
            let route_str = format!("{:?}", route);
            let short_route: Vec<String> = route_str
                .trim_matches(|c| c == '[' || c == ']')
                .split(", ")
                .map(|node_id| node_id.split('-').next().unwrap_or(node_id).to_string())
                .collect();
            let formatted_route = format!("[{}]", short_route.join(", "));
            println!("{:<60} {:<20}", formatted_route, format!("{:?}", message_type));
        }

        Ok(())
    }
}
