use std::{collections::HashMap, sync::mpsc::Receiver};

use flarch::{
    broker::{Broker, BrokerError},
    data_storage::DataStorageTemp,
    nodeids::U256,
    web_rtc::{node_connection::NCInput, WebRTCConnMessage},
};
use flmodules::{
    nodeconfig::{NodeConfig, NodeInfo},
    timer::TimerMessage,
    Modules,
};
use flmodules::network::messages::{NetworkIn, NetworkOut, NetworkMessage};

use flnode::node::{Node, NodeError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error(transparent)]
    Broker(#[from] BrokerError),
    #[error(transparent)]
    NodeData(#[from] NodeError),
}

pub struct NetworkSimul {
    pub nodes: HashMap<U256, NodeTimer>,
    pub messages: u64,
    node_brokers: HashMap<U256, Broker<NetworkMessage>>,
    node_taps: HashMap<U256, Receiver<NetworkMessage>>,
}

impl NetworkSimul {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            messages: 0,
            node_brokers: HashMap::new(),
            node_taps: HashMap::new(),
        }
    }

    pub async fn add_nodes(&mut self, modules: Modules, nbr: usize) -> Result<(), NetworkError> {
        for _ in 0..nbr {
            self.new_node(modules).await?;
        }
        self.send_update_list().await?;
        Ok(())
    }

    pub async fn tick(&mut self) -> Result<(), NetworkError> {
        for node in self.nodes.values_mut() {
            node.timer.settle_msg(TimerMessage::Second).await?;
        }
        Ok(())
    }

    // pub fn add_node(&mut self) {
    //     self.new_node();
    //     self.send_update_list();
    // }

    pub async fn send_update_list(&mut self) -> Result<(), NetworkError> {
        let list: Vec<NodeInfo> = self.nodes.values().map(|node| node.node_info()).collect();
        for id in self.nodes.keys() {
            if let Some(broker) = self.node_brokers.get_mut(id) {
                broker
                    .settle_msg(NetworkOut::NodeListFromWS(list.clone()).into())
                    .await?;
            }
        }
        Ok(())
    }

    async fn new_node(&mut self, modules: Modules) -> Result<(), NetworkError> {
        let mut broker = Broker::new();
        let node_timer = NodeTimer::new(broker.clone(), self.nodes.len() as u32, modules).await?;
        let id = node_timer.node.node_config.info.get_id();
        let (tap, _) = broker.get_tap_sync().await?;
        self.node_taps.insert(id, tap);
        self.node_brokers.insert(id, broker);
        self.nodes.insert(id, node_timer);
        Ok(())
    }

    /// tick collects all outgoing messages from the nodes and distributes them to the
    /// destination nodes.
    /// Then it calls `tick` on all nodes.
    pub async fn process(&mut self, nbr: usize) {
        for i in 0..nbr {
            if nbr > 1 {
                log::info!("Processing {}/{}", i + 1, nbr);
            }
            self.process_one().await;
        }
    }

    #[allow(dead_code)]
    pub async fn send_message(&mut self, id: &U256, bm: NetworkMessage) {
        if let Some(ch) = self.node_brokers.get_mut(id) {
            ch.settle_msg(bm).await.expect("Couldn't send message");
        }
    }

    async fn process_one(&mut self) {
        self.tick().await.unwrap();
        let ids: Vec<U256> = self.nodes.keys().cloned().collect();
        for id in ids.iter() {
            for msg in self.node_taps.get(id).unwrap().try_iter() {
                for (id_dst, msg_out) in self.process_msg(id, msg) {
                    if let Some(broker) = self.node_brokers.get_mut(&id_dst) {
                        log::trace!("Send: {} -> {}: {:?}", id, id_dst, msg_out);
                        broker
                            .settle_msg(msg_out)
                            .await
                            .expect("processing one message");
                        self.messages += 1;
                    }
                }
            }
        }

        for id in ids.iter() {
            self.nodes.get_mut(id).unwrap().process().await;
        }
    }

    fn process_msg(&self, id: &U256, msg: NetworkMessage) -> Vec<(U256, NetworkMessage)> {
        match msg {
            NetworkMessage::Input(NetworkIn::Connect(id_dst)) => {
                vec![
                    (*id, NetworkOut::Connected(id_dst).into()),
                    (id_dst, NetworkOut::Connected(*id).into()),
                ]
            }
            NetworkMessage::Input(NetworkIn::MessageToNode(from_id, msg_str)) => vec![(
                from_id,
                NetworkOut::MessageFromNode(id.clone(), msg_str).into(),
            )],
            NetworkMessage::WebRTC(WebRTCConnMessage::InputNC(
                id_dst,
                NCInput::Text(msg_node),
            )) => {
                vec![(
                    id_dst.clone(),
                    NetworkMessage::WebRTC(WebRTCConnMessage::InputNC(
                        *id,
                        NCInput::Text(msg_node.clone()).into(),
                    ))
                    .into(),
                )]
            }
            _ => vec![],
        }
    }
}

pub struct NodeTimer {
    pub node: Node,
    pub timer: Broker<TimerMessage>,
}

impl NodeTimer {
    pub async fn new(
        broker_net: Broker<NetworkMessage>,
        start: u32,
        modules: Modules,
    ) -> Result<Self, NodeError> {
        let mut node_config = NodeConfig::new();
        // Make sure the public key starts with a hex nibble corresponding to its position in the test.
        // This simplifies a lot debugging when there are multiple nodes.
        while format!("{:x}", node_config.info.get_id())
            .chars()
            .next()
            .unwrap()
            .to_digit(16)
            .unwrap()
            != (start % 16)
        {
            node_config = NodeConfig::new();
        }
        node_config.info.modules = modules;
        let mut node_data = Node::start(
            Box::new(DataStorageTemp::new()),
            node_config,
            broker_net.clone(),
            None,
        )
        .await?;
        let timer = Broker::new();
        node_data.add_timer(timer.clone()).await;

        Ok(Self {
            node: node_data,
            timer,
        })
    }

    pub fn node_info(&self) -> NodeInfo {
        self.node.node_config.info.clone()
    }

    pub async fn process(&mut self) {
        self.node
            .process()
            .await
            .err()
            .map(|e| log::error!("While processing node-data: {e:?}"));
    }

    #[allow(dead_code)]
    pub fn messages(&mut self) -> usize {
        self.node.gossip.as_ref().unwrap().chat_events().len()
    }
}
