use std::{collections::HashMap, fs::File, io::Write, ops::Range, path::PathBuf, sync::Arc};

use crate::{
    loopix::{
        broker::LoopixBroker,
        config::{LoopixConfig, LoopixRole},
        messages::LoopixMessage,
        storage::LoopixStorage,
    },
    network::messages::{NetworkIn, NetworkMessage, NetworkOut},
    nodeconfig::{NodeConfig, NodeInfo},
    overlay::{broker::loopix::OverlayLoopix, messages::OverlayMessage},
    web_proxy::{
        broker::{WebProxy, WebProxyError},
        core::WebProxyConfig,
    },
};
use flarch::{
    broker::{Broker, BrokerError},
    data_storage::DataStorageTemp,
    nodeids::NodeID,
    tasks::wait_ms,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{
    mpsc::UnboundedReceiver,
    oneshot::{channel, Sender},
};
use x25519_dalek::{PublicKey, StaticSecret};

/**
 * Create a LoopixNode with all the necessary configuration for one node.
 * It holds the different brokers which can be used from the outside.
 * `LoopixNode`s are created by `LoopixSetup` and held there.
 */
#[derive(Clone)]
pub struct LoopixNode {
    pub config: NodeConfig,
    pub net: Broker<NetworkMessage>,
    pub overlay: Broker<OverlayMessage>,
    // This one should not be used from the outside.
    pub loopix: Broker<LoopixMessage>,
    pub storage: Arc<LoopixStorage>,
}

impl LoopixNode {
    pub async fn new(loopix_cfg: LoopixConfig, config: NodeConfig) -> Result<Self, BrokerError> {
        let net = Broker::new();
        let loopix_broker = LoopixBroker::start(net.clone(), loopix_cfg, 1).await?;

        let overlay = OverlayLoopix::start(loopix_broker.broker.clone()).await?;
        Ok(Self {
            loopix: loopix_broker.broker,
            storage: loopix_broker.storage,
            config,
            net,
            overlay,
        })
    }
}

/**
 * I'm not sure I understood correctly how the clients, mixers, and providers can be
 * created using `LoopixConfig`. I gave it my best shot!
 *
 * The idea is that `LoopixSetup::new` creates the basic configuration, which is then
 * used to set up all the nodes. This is where I'm not sure wrt setting up the different
 * types of nodes.
 */
#[derive(Serialize, Deserialize)]
pub struct LoopixSetup {
    pub node_public_keys: HashMap<NodeID, PublicKey>,
    pub loopix_key_pairs: HashMap<NodeID, (PublicKey, StaticSecret)>,
    pub node_configs: HashMap<NodeID, NodeConfig>,
    pub path_length: usize,
    pub all_nodes: Vec<NodeInfo>,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub clients: Vec<LoopixNode>,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub mixers: Vec<LoopixNode>,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub providers: Vec<LoopixNode>,
}

impl LoopixSetup {
    pub async fn new(path_length: usize) -> Result<Self, BrokerError> {

        let (node_infos, node_public_keys, loopix_key_pairs, node_configs) = Self::create_nodes_and_keys(path_length);

        log::info!("Node configs: {:?}", node_configs.len());
        log::info!("Node key pairs: {:?}", loopix_key_pairs.len());

        let mut setup = Self {
            node_public_keys,
            loopix_key_pairs,
            node_configs,
            path_length,
            all_nodes: node_infos,
            clients: vec![],
            mixers: vec![],
            providers: vec![],
        };

        setup.clients = setup
            .get_nodes(0..path_length, LoopixRole::Client)
            .await?;
        setup.providers = setup
            .get_nodes(path_length..2 * path_length, LoopixRole::Provider)
            .await?;
        setup.mixers = setup
            .get_nodes(2 * path_length..(path_length * path_length + path_length * 2), LoopixRole::Mixnode)
            .await?;

        // log::info!("Number of mixes, providers, clients: {:?}, {:?}, {:?}", setup.mixers.len(), setup.providers.len(), setup.clients.len());

        Ok(setup)
    }

    pub fn create_nodes_and_keys(path_length: usize) -> (Vec<NodeInfo>, HashMap<NodeID, PublicKey>, HashMap<NodeID, (PublicKey, StaticSecret)>, HashMap<NodeID, NodeConfig>) {
        let mut node_infos = Vec::new();
        let mut node_public_keys = HashMap::new();
        let mut loopix_key_pairs = HashMap::new();
        let mut node_configs = HashMap::new();

        for _ in 0..path_length * path_length + path_length * 2 {
            let node_config = NodeConfig::new();
            let node_id = node_config.clone().info.get_id();

            node_infos.push(node_config.clone().info);
            node_configs.insert(node_id, node_config.clone());

            let (public_key, private_key) = LoopixStorage::generate_key_pair();
            node_public_keys.insert(node_id, public_key);
            loopix_key_pairs.insert(node_id, (public_key, private_key));
        }

        (node_infos, node_public_keys, loopix_key_pairs, node_configs)
    }

    pub async fn save_storages(&self, path: PathBuf) {
        if let Err(e) = std::fs::create_dir_all(&path) {
            println!("Failed to create directory: {}", e);
        }

        for (node_type, nodes) in [("client", &self.clients), ("mixer", &self.mixers), ("provider", &self.providers)].iter() {
            for (index, node) in nodes.iter().enumerate() {
                let mut storage_path = path.clone();
                storage_path.push(format!("{}_{}.yaml", node_type, index));

                let storage_bytes = node.storage.to_yaml_async().await.unwrap();

                match File::create(&storage_path) {
                Ok(mut file) => {
                    if let Err(e) = file.write_all(storage_bytes.as_bytes()) {
                        println!("Failed to write storage file: {}", e);
                    } else {
                        println!("Saved storage file to: {:?}", storage_path);
                    }
                }
                Err(e) => {
                    println!("Failed to create storage file: {}", e);
                    }
                }
            }
        }
    }

    pub async fn get_nodes(
        &self,
        range: Range<usize>,
        role: LoopixRole,
    ) -> Result<Vec<LoopixNode>, BrokerError> {
        let mut ret = vec![];
        // log::info!("Getting nodes for range: {:?} and role: {:?}", range, role);
        for i in range {
            // log::info!("i: {} and getting node: {:?}", i, self.all_nodes[i].get_id());
            ret.push(self.get_node(self.all_nodes[i].get_id(), role.clone()).await?);
        }
        Ok(ret)
    }

    pub async fn get_node(
        &self,
        node_id: NodeID,
        role: LoopixRole,
    ) -> Result<LoopixNode, BrokerError> {
        let loopix_cfg = self.get_config(node_id, role).await?;
        LoopixNode::new(loopix_cfg, self.node_configs.get(&node_id).unwrap().clone()).await
    }

    pub async fn get_config(
        &self,
        node_id: NodeID,
        role: LoopixRole,
    ) -> Result<LoopixConfig, BrokerError> {
        let private_key = &self.loopix_key_pairs.get(&node_id).unwrap().1;
        let public_key = &self.loopix_key_pairs.get(&node_id).unwrap().0;

        let config = LoopixConfig::default_with_path_length(
            role,
            node_id,
            self.path_length as usize,
            private_key.clone(),
            public_key.clone(),
            self.all_nodes.clone(),
        );

        config
            .storage_config
            .set_node_public_keys(self.node_public_keys.clone())
            .await;

        Ok(config)
    }

    pub async fn print_all_messages(&self, print_full_id: bool) {
        println!();
        println!();
        println!("\n{:*<300}", "");
        println!("Network Configurations:");
        println!("Number of mixes, providers, clients: {:?}, {:?}, {:?}", self.mixers.len(), self.providers.len(), self.clients.len());
        println!("{:<30} {:<30} {:<30}", "Clients", "Mixers", "Providers");

        for i in 0..self.mixers.len() {
            let default_id = NodeID::from(0u32);
            let client_id = self.clients.get(i).map(|node| node.config.info.get_id()).unwrap_or(default_id);
            let mixer_id = self.mixers.get(i).map(|node| node.config.info.get_id()).unwrap_or(default_id);
            let provider_id = self.providers.get(i).map(|node| node.config.info.get_id()).unwrap_or(default_id);
            println!("{:<30} {:<30} {:<30}", client_id, mixer_id, provider_id);
        }
        println!("\n{:*<300}", "");


        for node in self.clients.iter() {
            println!("\n{:*<300}", "");
            self.print_node_messages(node, "Client", print_full_id).await;
            println!("\n{:*<300}", "");
        }
        for node in self.mixers.iter() {
            println!("\n{:*<300}", "");
            self.print_node_messages(node, "Mixer", print_full_id).await;
            println!("\n{:*<300}", "");
        }
        for node in self.providers.iter() {
            println!("\n{:*<300}", "");
            self.print_node_messages(node, "Provider", print_full_id).await;
            println!("\n{:*<300}", "");
        }
    }

    async fn print_node_messages(&self, node: &LoopixNode, role: &str, _print_full_id: bool) {
        println!();
        
        let node_id = node.config.info.get_id();
        println!("Node: {:x} ({role})", node_id);

        let received_messages = node.storage.get_received_messages().await;
        println!("Received messages: {:?}", received_messages.len());

        let forwarded_messages = node.storage.get_forwarded_messages().await;
        println!("Forwarded messages: {:?}", forwarded_messages.len());

        let sent_messages = node.storage.get_sent_messages().await;
        println!("Sent messages: {:?}", sent_messages.len());

        println!();
        println!("\nForwarded Messages:");
        println!("{:<10} {:<20} {:<20} {:<20} {:<20}", "Count", "Timestamp", "From -> To", "Message ID", "Message ID");
        println!("{:-<300}", "");

        let mut forwarded_count = HashMap::new();
        for (_timestamp, from, to, message_id) in forwarded_messages {
            *forwarded_count.entry((from, to, message_id)).or_insert(0) += 1;
        }
        for ((from, to, message_id), count) in forwarded_count {
            println!("{:<10} {:<60} {:<20} {:<20}", count, format!("{:x} -> {:x}", from, to), "N/A", format!("{:?}", message_id));
        }

        println!("\nReceived Messages:");
        println!("{:<10} {:<20} {:<20} {:<20} {:<20} {:<20}", "Count", "Timestamp", "Origin -> Relayed By", "Message Type", "Message ID", "Message ID");
        println!("{:-<300}", "");

        let mut received_count = HashMap::new();
        for (_timestamp, origin, relay, message_type, message_id) in received_messages {
            *received_count.entry((origin, relay, message_type, message_id)).or_insert(0) += 1;
        }
        for ((origin, relay, message_type, message_id), count) in received_count {
            println!("{:<10} {:<20} {:<40} {:<20} {:<20}", count, "N/A", format!("{:x} -> {:x}", origin, relay), format!("{:?}", message_type), format!("{:?}", message_id));
        }

        println!("\nSent Messages:");
        println!("{:<10} {:<20} {:<100} {:<60} {:<20} {:<20}", "Count", "Timestamp", "Message Type", "Route", "Message ID", "Message ID");
        println!("{:-<300}", "");

        let mut sent_count = HashMap::new();
        for (_timestamp, route, message_type, message_id) in sent_messages {
            let short_route: Vec<String> = route
                .iter()
                .map(|node_id| format!("{:x}", node_id))
                .collect::<Vec<String>>();
            *sent_count.entry((short_route, message_type, message_id)).or_insert(0) += 1;
        }
        for ((short_route, message_type, message_id), count) in sent_count {
            println!("{:<10} {:<20} {:<100} {:<60}", count, format!("{:?}", message_type), format!("[{}]", short_route.join(", ")), format!("{:?}", message_id));
        }

        if role == "Provider" {
            let client_messages = node.storage.get_all_client_messages().await;
            println!("\nClient Messages:");
            println!("{:<60} {:<20} {:<20} {:<20}", "Client ID", "Message Type", "Message ID", "Message ID");
            println!("{:-<300}", "");
            for (client_id, messages) in client_messages {
                for (sphinx, timestamp) in messages {
                    println!("{:<60} {:<20} {:<20} {:<20}", format!("{:x}", client_id), format!("{:?}", sphinx), format!("{:?}", sphinx.message_id.clone()), format!("{:?}", timestamp));
                }
            }
        }
    }
    
}

/**
 * A very simple network simulator which connects different `Broker<NetworkMessage>`s
 * together using a `process` method which will handle one message from each broker.
 */
pub struct NetworkSimul {
    pub nodes: HashMap<NodeID, (Broker<NetworkMessage>, UnboundedReceiver<NetworkMessage>)>,
}

impl NetworkSimul {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    pub async fn add_nodes(&mut self, nodes: Vec<LoopixNode>) -> Result<(), BrokerError> {
        for mut node in nodes {
            self.nodes.insert(
                node.config.info.get_id(),
                (node.net.clone(), node.net.get_tap().await?.0),
            );
        }
        Ok(())
    }

    pub fn process(&mut self) -> Result<usize, BrokerError> {
        let mut msgs = vec![];
        for (&id_tx, (_, rx)) in self.nodes.iter_mut() {
            if let Ok(msg) = rx.try_recv() {
                if let NetworkMessage::Input(NetworkIn::MessageToNode(id_rx, node_msg)) = msg {
                    msgs.push((id_tx, id_rx, node_msg));
                }
            }
        }

        for (id_tx, id_rx, msg) in msgs.iter() {
            if let Some((broker, _)) = self.nodes.get_mut(&id_rx) {
                // This is for debugging and can be removed
                // println!("NetworkOut: {id_tx}->{id_rx}: {msg}");
                broker.emit_msg(NetworkMessage::Output(NetworkOut::MessageFromNode(
                    id_tx.clone(),
                    msg.clone(),
                )))?;
            }
        }

        Ok(msgs.len())
    }

    pub fn process_loop(mut self) -> Sender<bool> {
        let (tx, mut rx) = channel::<bool>();
        tokio::spawn(async move {
            // Loop over all messages from the loopix-nodes and pass them between the nodes.
            loop {
                match self.process() {
                    Ok(msgs) => {
                        if msgs == 0 {
                            // Wait for 100 ms if no messages got passed.
                            wait_ms(100).await;
                        }
                    }
                    Err(e) => println!("Error while processing network: {e:?}"),
                }
                if rx.try_recv().is_ok() {
                    println!("Terminating loop");
                    return;
                }
            }
        });
        tx
    }

}

/**
 * Once the main Loopix part works, you can try the ProxyBroker.
 * `WebProxy` needs the `OverlayOut::NodeInfosConnected` to get
 * a list of potential nodes where it can send a request to.
 * If this message is never received, it will panic.
 */
pub struct ProxyBroker {
    pub id: NodeID,
    pub proxy: WebProxy,
}

impl ProxyBroker {
    pub async fn new(loopix: Broker<LoopixMessage>) -> Result<Self, WebProxyError> {
        let ds = DataStorageTemp::new();
        let id = NodeID::rnd();
        Ok(Self {
            proxy: WebProxy::start(
                Box::new(ds),
                id,
                OverlayLoopix::start(loopix).await?,
                WebProxyConfig::default(),
            )
            .await?,
            id,
        })
    }
}
