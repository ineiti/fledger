use crate::broker::Subsystem;
use crate::node::logic::messages::Message;
use crate::node::logic::messages::MessageV1;
use crate::node::{logic::messages::NodeMessage, network::BrokerNetwork};
use crate::{
    broker::{BInput, BrokerMessage, SubsystemListener},
    node::{
        config::{NodeConfig, NodeInfo},
        node_data::NodeData,
        timer::BrokerTimer,
    },
};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::sync::Mutex;

use message::network::{MessageFromNetwork, MessageToNetwork};

/// Links with the modules-crate by sending and receiving messages from it.
/// One thing to look out for is how to handle the Broker structure - it's
/// now only used in the common-crate, but not in the modules-crate.
pub struct Modules {
    _node_data: Arc<Mutex<NodeData>>,
    _node_config: NodeConfig,
    broker_tx: Sender<BInput>,
    module: message::network::Network,
}

impl Modules {
    pub fn new(node_data: Arc<Mutex<NodeData>>) {
        let (node_config, mut broker, ds) = {
            let mut nd = node_data.lock().unwrap();
            let gossip_msgs_str = nd
                .storage
                .get(message::network::STORAGE_GOSSIP_CHAT)
                .get(message::network::STORAGE_GOSSIP_CHAT)
                .unwrap();
            if gossip_msgs_str != "" {
                if let Err(e) = nd.gossip_messages.load(&gossip_msgs_str){
                    log::warn!("Couldn't load gossip messages: {}", e);
                }
            } else {
                let msgs = nd
                    .messages
                    .storage
                    .values()
                    .map(|msg| raw::gossip_chat::text_message::TextMessage {
                        src: msg.src,
                        created: msg.created,
                        msg: msg.msg.clone(),
                    })
                    .collect();
                nd.gossip_messages.add_messages(msgs);
            }
            (
                nd.node_config.clone(),
                nd.broker.clone(),
                nd.storage.clone(),
            )
        };
        let module = message::network::Network::new(ds);
        broker
            .add_subsystem(Subsystem::Handler(Box::new(Modules {
                _node_data: node_data,
                _node_config: node_config,
                broker_tx: broker.clone_tx(),
                module,
            })))
            .unwrap();
    }

    fn tick(&mut self) {
        let msgs = self.module.tick();
        self.process_msgs(msgs);
    }

    fn update_list(&mut self, ul: Vec<NodeInfo>) {
        let msgs = self
            .module
            .process_message(MessageFromNetwork::AvailableNodes(
                ul.iter().map(|ni| ni.get_id()).collect(),
            ));
        self.process_msgs(msgs);
    }

    fn node_msg(&mut self, nm: NodeMessage) {
        if let Message::V1(MessageV1::ModuleMessage(msg_str)) = nm.msg {
            let msgs = self
                .module
                .process_message(MessageFromNetwork::Msg(nm.id, msg_str));
            self.process_msgs(msgs);
        }
    }

    fn process_msgs(&mut self, msgs: Vec<MessageToNetwork>) {
        for msg in msgs {
            let bm = match msg.clone() {
                MessageToNetwork::Connect(id) => Some(BrokerNetwork::WebRTC(
                    id,
                    Message::V1(MessageV1::Ping()).to_string().unwrap(),
                )),
                MessageToNetwork::Disconnect(_) => {
                    log::warn!("Disconnect is not implemented yet");
                    None
                }
                MessageToNetwork::Msg(id, msg) => Some(BrokerNetwork::WebRTC(
                    id,
                    Message::V1(MessageV1::ModuleMessage(msg))
                        .to_string()
                        .unwrap(),
                )),
            };
            if let Some(msg_bm) = bm {
                if let Err(e) = self
                    .broker_tx
                    .send(BInput::BM(BrokerMessage::Network(msg_bm)))
                {
                    log::warn!("Couldn't send {:?}: {}", msg, e);
                }
            }
        }
    }
}

impl SubsystemListener for Modules {
    fn messages(&mut self, msgs: Vec<&BrokerMessage>) -> Vec<BInput> {
        for msg in msgs {
            match msg {
                BrokerMessage::Network(BrokerNetwork::UpdateList(ul)) => {
                    self.update_list(ul.clone())
                }
                BrokerMessage::NodeMessage(nm) => self.node_msg(nm.clone()),
                BrokerMessage::Timer(BrokerTimer::Second) => self.tick(),
                _ => {}
            }
        }
        vec![]
    }
}
