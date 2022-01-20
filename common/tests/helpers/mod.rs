use common::{
    broker::{BInput, BrokerMessage, Subsystem, SubsystemListener},
    node::{
        config::{NodeConfig, NodeInfo},
        modules::{gossip_chat::GossipChat, random_connections::RandomConnections},
        network::BrokerNetwork,
        node_data::NodeData,
    },
};
use std::{
    collections::HashMap,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc, Mutex,
    },
};
use types::{data_storage::TempDSB, nodeids::U256};

pub struct Network {
    nodes: HashMap<U256, Node>,
    node_inputs: HashMap<U256, Sender<BrokerNetwork>>,
    node_outputs: HashMap<U256, Receiver<BrokerNetwork>>,
}

impl Network {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            node_inputs: HashMap::new(),
            node_outputs: HashMap::new(),
        }
    }

    pub fn add_nodes(&mut self, nbr: usize) {
        for _ in 0..nbr {
            self.new_node();
        }
        self.send_update_list();
    }

    // pub fn add_node(&mut self) {
    //     self.new_node();
    //     self.send_update_list();
    // }

    pub fn send_update_list(&mut self) {
        let list: Vec<NodeInfo> = self.nodes.values().map(|node| node.node_info()).collect();
        for id in self.nodes.keys() {
            self.node_inputs
                .get_mut(id)
                .and_then(|ch| ch.send(BrokerNetwork::UpdateList(list.clone())).ok());
        }
    }

    fn new_node(&mut self) {
        let (node_in_snd, node_in_rcv) = channel();
        let (node_out_snd, node_out_rcv) = channel();
        let node = Node::new(node_out_snd, node_in_rcv);
        let id = {
            let nd = node.node_data.lock().unwrap();
            nd.node_config.our_node.get_id()
        };
        self.node_inputs.insert(id, node_in_snd);
        self.node_outputs.insert(id, node_out_rcv);
        self.nodes.insert(id, node);
    }

    /// tick collects all outgoing messages from the nodes and distributes them to the
    /// destination nodes.
    /// Then it calls `tick` on all nodes.
    pub fn process(&mut self, nbr: usize) {
        for i in 0..nbr {
            log::info!("Processing {}/{}", i + 1, nbr);
            self.process_one();
        }
    }

    fn process_one(&mut self) {
        let ids: Vec<U256> = self.nodes.keys().cloned().collect();
        for id in ids.iter() {
            for msg in self.node_outputs.get(id).unwrap().try_iter() {
                log::debug!("node2node message: {}: {:?}", id, msg);
                if let BrokerNetwork::WebRTC(dest, _) = msg {
                    if let Some(ch_in) = self.node_inputs.get(&dest) {
                        ch_in.send(msg).unwrap();
                    }
                }
            }
        }

        for id in ids.iter() {
            self.nodes.get_mut(id).unwrap().process();
        }
    }
}

pub struct Node {
    node_data: Arc<Mutex<NodeData>>,
    rcv: Receiver<BrokerNetwork>,
}

impl Node {
    pub fn new(snd: Sender<BrokerNetwork>, rcv: Receiver<BrokerNetwork>) -> Self {
        let node_data = NodeData::new(NodeConfig::new(), TempDSB::new());
        RandomConnections::start(Arc::clone(&node_data));
        GossipChat::start(Arc::clone(&node_data));
        WebRTC::start(Arc::clone(&node_data), snd);

        Self { node_data, rcv }
    }

    pub fn node_info(&self) -> NodeInfo {
        self.node_data.lock().unwrap().node_config.our_node.clone()
    }

    pub fn process(&mut self) {
        let mut broker = { self.node_data.lock().unwrap().broker.clone() };
        for msg in self.rcv.try_iter() {
            broker.enqueue_bm(BrokerMessage::Network(msg));
        }
        if let Err(e) = broker.process() {
            log::error!("While processing: {}", e);
        }
    }
}

pub struct WebRTC {
    snd: Sender<BrokerNetwork>,
}

impl WebRTC {
    pub fn start(node_data: Arc<Mutex<NodeData>>, snd: Sender<BrokerNetwork>) {
        node_data
            .lock()
            .unwrap()
            .broker
            .add_subsystem(Subsystem::Handler(Box::new(Self { snd })))
            .unwrap();
    }

    fn msg_outgoing(&mut self, msg: &BrokerNetwork) -> Option<BrokerNetwork> {
        match msg {
            BrokerNetwork::WebRTC(_, _) => {
                self.snd.send(msg.clone()).unwrap();
                None
            }
            BrokerNetwork::Connect(id) => Some(BrokerNetwork::Connected(*id)),
            BrokerNetwork::Disconnect(id) => Some(BrokerNetwork::Disconnected(*id)),
            _ => None,
        }
    }
}

impl SubsystemListener for WebRTC {
    fn messages(&mut self, msgs: Vec<&BrokerMessage>) -> Vec<BInput> {
        msgs.iter()
            .filter_map(|msg| match msg {
                BrokerMessage::Network(bn) => self.msg_outgoing(bn),
                _ => None,
            })
            .map(|bn| BInput::BM(BrokerMessage::Network(bn)))
            .collect()
    }
}
