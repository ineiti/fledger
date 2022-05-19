use std::{collections::HashMap, sync::mpsc::Receiver};

use flmodules::timer::TimerMessage;
use flnet::{
    config::{NodeConfig, NodeInfo},
    network::{
        node_connection::{NCInput, NCMessage},
        NetCall, NetReply, NetworkMessage,
    },
};
use flutils::{
    broker::{Broker, BrokerError},
    data_storage::TempDSB,
    nodeids::U256,
};

use flnode::node_data::{NodeData, NodeDataError, Brokers};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error(transparent)]
    Broker(#[from] BrokerError),
    #[error(transparent)]
    NodeData(#[from] NodeDataError),
}

pub struct Network {
    pub nodes: HashMap<U256, Node>,
    pub messages: u64,
    node_brokers: HashMap<U256, Broker<NetworkMessage>>,
    node_taps: HashMap<U256, Receiver<NetworkMessage>>,
}

impl Network {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            messages: 0,
            node_brokers: HashMap::new(),
            node_taps: HashMap::new(),
        }
    }

    pub async fn add_nodes(&mut self, brokers: Brokers, nbr: usize) -> Result<(), NetworkError> {
        for _ in 0..nbr {
            self.new_node(brokers).await?;
        }
        self.send_update_list().await?;
        Ok(())
    }

    pub async fn tick(&mut self) -> Result<(), NetworkError>{
        for node in self.nodes.values_mut(){
            node.timer.emit_msg(TimerMessage::Second).await?;
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
                    .emit_msg(NetReply::RcvWSUpdateList(list.clone()).into())
                    .await?;
            }
        }
        Ok(())
    }

    async fn new_node(&mut self, brokers: Brokers) -> Result<(), NetworkError> {
        let mut broker = Broker::new();
        let node = Node::new(broker.clone(), self.nodes.len() as u32, brokers).await?;
        let id = node.node_data.node_config.our_node.get_id();
        let (tap, _) = broker.get_tap().await?;
        self.node_taps.insert(id, tap);
        self.node_brokers.insert(id, broker);
        self.nodes.insert(id, node);
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
            ch.emit_msg(bm).await.expect("Couldn't send message");
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
                            .emit_msg(msg_out)
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
            NetworkMessage::Call(NetCall::Connect(id_dst)) => {
                vec![
                    (*id, NetReply::Connected(id_dst).into()),
                    (id_dst, NetReply::Connected(*id).into()),
                ]
            }
            NetworkMessage::Call(NetCall::SendNodeMessage((from_id, msg_str))) => vec![(
                from_id,
                NetReply::RcvNodeMessage((id.clone(), msg_str)).into(),
            )],
            NetworkMessage::NodeConnection((id_dst, NCMessage::Input(NCInput::Text(msg_node)))) => {
                vec![(
                    id_dst.clone(),
                    NetworkMessage::NodeConnection((*id, NCInput::Text(msg_node.clone()).into()))
                        .into(),
                )]
            }
            _ => vec![],
        }
    }
}

pub struct Node {
    pub node_data: NodeData,
    pub timer: Broker<TimerMessage>,
}

impl Node {
    pub async fn new(
        broker_net: Broker<NetworkMessage>,
        start: u32,
        brokers: Brokers,
    ) -> Result<Self, NodeDataError> {
        let mut node_config = NodeConfig::new();
        while format!("{:x}", node_config.our_node.get_id())
            .chars()
            .next()
            .unwrap()
            .to_digit(16)
            .unwrap()
            != (start % 16)
        {
            node_config = NodeConfig::new();
        }
        let mut node_data = NodeData::start_some(TempDSB::new(), node_config, broker_net.clone(), brokers).await?;
        let timer = Broker::new();
        node_data.add_timer(timer.clone()).await;

        Ok(Self {
            node_data,
            timer,
        })
    }

    pub fn node_info(&self) -> NodeInfo {
        self.node_data.node_config.our_node.clone()
    }

    pub async fn process(&mut self) {
        self.node_data
            .process()
            .await
            .err()
            .map(|e| log::error!("While processing node-data: {e:?}"));
    }

    #[allow(dead_code)]
    pub fn messages(&mut self) -> usize {
        self.node_data.gossip.chat_events().len()
    }
}
