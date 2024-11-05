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
    overlay::messages::{NetworkWrapper, OverlayIn, OverlayMessage, OverlayOut},
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
        overlay: Broker<OverlayMessage>,
        network: Broker<NetworkMessage>,
        config: LoopixConfig,
    ) -> Result<Broker<LoopixMessage>, BrokerError> {
        let mut broker = Broker::new();

        broker
            .link_bi(
                overlay.clone(),
                Box::new(Self::from_overlay),
                Box::new(Self::to_overlay),
            )
            .await?;

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
        ) = channel(100);
        let (overlay_sender, overlay_receiver): (Sender<(NodeID, NetworkWrapper)>, Receiver<(NodeID, NetworkWrapper)>) =
            channel(100);

        let loopix_messages = LoopixMessages::new(
            node_type.arc_clone(),
            network_sender.clone(),
            overlay_sender.clone(),
        );

        broker
            .add_subsystem(Subsystem::Handler(Box::new(LoopixTranslate {
                _overlay: overlay.clone(),
                _network: network.clone(),
                _loopix_messages: loopix_messages.clone(),
            })))
            .await?;

        // Threads for all nodes
        Self::start_network_send_thread(loopix_messages.clone(), network.clone(), network_receiver);
        
        Self::start_overlay_send_thread(overlay.clone(), overlay_receiver);
        
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

        Ok(broker)
    }

    fn from_overlay(msg: OverlayMessage) -> Option<LoopixMessage> {
        match msg {
            OverlayMessage::Input(OverlayIn::NetworkWrapperToNetwork(node_id, wrapper)) => {
                Some(LoopixIn::OverlayRequest(node_id, wrapper).into())
            }

            //TODO other overlay messages
            _ => None,
        }
    }

    fn from_network(msg: NetworkMessage) -> Option<LoopixMessage> {
        if let NetworkMessage::Output(NetworkOut::MessageFromNode(_node_id, message)) = msg {
            // TODO: probably node_id should be used somewhere.
            let sphinx_packet: Sphinx = serde_yaml::from_str(&message).unwrap();
            return Some(LoopixIn::SphinxFromNetwork(sphinx_packet).into());
        }
        None
    }

    fn to_overlay(msg: LoopixMessage) -> Option<OverlayMessage> {
        if let LoopixMessage::Output(LoopixOut::OverlayReply(destination, module_msg)) = msg {
            return Some(OverlayOut::NetworkWrapperFromNetwork(destination, module_msg).into());
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
        let loop_rate = Duration::from_secs_f64(loopix_messages.role.get_config().lambda_loop());

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(loop_rate).await;
                loopix_messages.send_loop_message().await;
            }
        });
    }

    pub fn start_drop_message_thread(loopix_messages: LoopixMessages) {
        let drop_rate = Duration::from_secs_f64(loopix_messages.role.get_config().lambda_drop());

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(drop_rate).await;
                loopix_messages.send_drop_message().await;
            }
        });
    }

    pub fn start_overlay_send_thread(mut overlay: Broker<OverlayMessage>, mut receiver: Receiver<(NodeID, NetworkWrapper)>) {
        tokio::spawn(async move {
            loop {
                if let Some((node_id, wrapper)) = receiver.recv().await {
                    if let Err(e) = overlay.emit_msg(OverlayOut::NetworkWrapperFromNetwork(node_id, wrapper).into()) {
                        log::error!("Error emitting overlay message: {e:?}");
                    }
                }
            }
        });
    }

    pub fn start_network_send_thread(loopix_messages: LoopixMessages, mut network: Broker<NetworkMessage>, mut receiver: Receiver<(NodeID, Delay, Sphinx)>) {
        tokio::spawn(async move {
            let mut sphinx_messages: Vec<(NodeID, Duration, Sphinx)> = Vec::new();
            let payload_rate =
                Duration::from_secs_f64(loopix_messages.role.get_config().lambda_payload());

            loop {
                // Wait for send delay
                tokio::time::sleep(payload_rate).await;

                // Subtract the wait duration from all message delays
                for (_, delay, _) in &mut sphinx_messages {
                    *delay = delay.saturating_sub(payload_rate);
                }

                // Receive new messages
                if let Some((node_id, delay, sphinx)) = receiver.recv().await {
                    sphinx_messages.push((node_id, delay.to_duration(), sphinx));
                }

                // Sort messages by remaining delay
                sphinx_messages.sort_by_key(|&(_, delay, _)| delay); // TODO technically this is not the protocol

                // Emit messages with 0 or less delay
                if let Some((node_id, delay, sphinx_packet)) = sphinx_messages.first() {
                    if *delay <= Duration::ZERO {
                        let msg = serde_yaml::to_string(&sphinx_packet).unwrap();
                        if let Err(e) =
                            network.emit_msg(NetworkIn::MessageToNode(*node_id, msg).into())
                        {
                            log::error!("Error emitting networkmessage: {e:?}");
                        } else {
                            sphinx_messages.remove(0);
                        }
                    } else {
                        let (node_id, sphinx) = loopix_messages.create_drop_message().await;
                        let msg = serde_yaml::to_string(&sphinx).unwrap();
                        if let Err(e) =
                            network.emit_msg(NetworkIn::MessageToNode(node_id, msg).into())
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
    _overlay: Broker<OverlayMessage>,
    _network: Broker<NetworkMessage>,
    _loopix_messages: LoopixMessages,
}

#[platform_async_trait()]
impl SubsystemHandler<LoopixMessage> for LoopixTranslate {
    async fn messages(&mut self, _msgs: Vec<LoopixMessage>) -> Vec<LoopixMessage> {
        let outgoing_msgs = vec![];

        // for msg in msgs {
        //     if let LoopixMessage::Input(loopix_in) = msg {
        //         let processed_msgs = self.loopix_messages.process_messages(vec![loopix_in]);
        //         outgoing_msgs.extend(processed_msgs.into_iter().map(LoopixMessage::Output));
        //     }
        // }

        // for msg in msgs {
        //     match msg {
        //         LoopixMessage::Input(loopix_in) => {
        //             let processed_msgs = self.loopix_messages.process_messages(vec![loopix_in]);
        //             outgoing_msgs.extend(processed_msgs.into_iter().map(LoopixMessage::Output));
        //         }
        //         LoopixMessage::Output(LoopixOut::OverlayReply(node_id, module_msg)) => {
        //             self.overlay.emit_msg(OverlayOut::NetworkWrapperFromNetwork(node_id, module_msg).into()).unwrap();
        //         }
        //         LoopixMessage::Output(LoopixOut::SphinxToNetwork(node_id, sphinx)) => {
        //             let msg = serde_json::to_string(&sphinx).unwrap();
        //             self.network.emit_msg(NetworkIn::SendLoopixMessage(node_id,
        //                 msg).into()).unwrap();
        //         }
        //     }
        // }

        outgoing_msgs
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::loopix::config::{LoopixConfig, LoopixRole};
    use crate::loopix::storage::LoopixStorage;
    use crate::network::messages::NetworkMessage;
    use crate::overlay::messages::OverlayMessage;
    use flarch::broker::Broker;

    #[tokio::test]
    async fn create_broker() {
        let path_length = 2;

        // set up network
        let mut node_public_keys = HashMap::new();
        let mut node_key_pairs = HashMap::new();

        for mix in 0..path_length * path_length + path_length + path_length {
            let node_id = NodeID::from(mix as u32);
            let (public_key, private_key) = LoopixStorage::generate_key_pair();
            node_public_keys.insert(node_id, public_key);
            node_key_pairs.insert(node_id, (public_key, private_key));
        }

        let node_id = 1;
        let private_key = &node_key_pairs.get(&NodeID::from(node_id)).unwrap().1;
        let public_key = &node_key_pairs.get(&NodeID::from(node_id)).unwrap().0;

        let config = LoopixConfig::default_with_path_length(
            LoopixRole::Client,
            node_id,
            path_length,
            private_key.clone(),
            public_key.clone(),
        );

        config
            .storage_config
            .set_node_public_keys(node_public_keys)
            .await;

        let overlay = Broker::<OverlayMessage>::new();
        let network = Broker::<NetworkMessage>::new();

        let result = LoopixBroker::start(overlay, network, config).await;
        assert!(result.is_ok());
    }
}
