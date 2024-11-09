use std::{collections::HashMap, ops::Range};

use crate::{
    loopix::{
        broker::LoopixBroker,
        config::{LoopixConfig, LoopixRole},
        messages::LoopixMessage,
        storage::LoopixStorage,
    },
    network::messages::{NetworkIn, NetworkMessage, NetworkOut},
    nodeconfig::NodeConfig,
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
}

impl LoopixNode {
    pub async fn new(loopix_cfg: LoopixConfig) -> Result<Self, BrokerError> {
        let config = NodeConfig::new();
        let net = Broker::new();
        let overlay = Broker::new();
        Ok(Self {
            loopix: LoopixBroker::start(overlay.clone(), net.clone(), loopix_cfg).await?,
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
    pub node_key_pairs: HashMap<NodeID, (PublicKey, StaticSecret)>,
    pub path_length: u32,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub clients: Vec<LoopixNode>,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub mixers: Vec<LoopixNode>,
    #[serde(skip_serializing, skip_deserializing, default)]
    pub providers: Vec<LoopixNode>,
}

impl LoopixSetup {
    pub async fn new(path_length: u32) -> Result<Self, BrokerError> {
        // set up network
        let mut node_public_keys = HashMap::new();
        let mut node_key_pairs = HashMap::new();

        for mix in 0..path_length * path_length + path_length + path_length {
            let node_id = NodeID::from(mix);
            let (public_key, private_key) = LoopixStorage::generate_key_pair();
            node_public_keys.insert(node_id, public_key);
            node_key_pairs.insert(node_id, (public_key, private_key));
        }

        let mut setup = Self {
            node_public_keys,
            node_key_pairs,
            path_length,
            clients: vec![],
            mixers: vec![],
            providers: vec![],
        };

        // TODO: correct attribution of NodeIDs to client|mixer|provider
        // TODO: using u32 as NodeID is very confusing and will not work in a real setup!
        setup.clients = setup.get_nodes(0..path_length, LoopixRole::Client).await?;
        setup.providers = setup
            .get_nodes(path_length..2 * path_length, LoopixRole::Provider)
            .await?;
        setup.mixers = setup
            .get_nodes(2 * path_length..4 * path_length, LoopixRole::Mixnode)
            .await?;

        Ok(setup)
    }

    pub async fn get_nodes(
        &self,
        range: Range<u32>,
        role: LoopixRole,
    ) -> Result<Vec<LoopixNode>, BrokerError> {
        let mut ret = vec![];
        for id in range {
            ret.push(self.get_node(id, role.clone()).await?);
        }
        Ok(ret)
    }

    pub async fn get_node(
        &self,
        node_id: u32,
        role: LoopixRole,
    ) -> Result<LoopixNode, BrokerError> {
        LoopixNode::new(self.get_config(node_id, role).await?).await
    }

    pub async fn get_config(
        &self,
        node_id: u32,
        role: LoopixRole,
    ) -> Result<LoopixConfig, BrokerError> {
        let private_key = &self.node_key_pairs.get(&NodeID::from(node_id)).unwrap().1;
        let public_key = &self.node_key_pairs.get(&NodeID::from(node_id)).unwrap().0;

        let config = LoopixConfig::default_with_path_length(
            role,
            node_id,
            self.path_length as usize,
            private_key.clone(),
            public_key.clone(),
        );

        config
            .storage_config
            .set_node_public_keys(self.node_public_keys.clone())
            .await;

        Ok(config)
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
                println!("{id_tx}->{id_rx}: {msg}");
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
