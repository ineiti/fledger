use std::{collections::HashMap, error::Error, ops::Range};

use flarch::{
    broker::{Broker, BrokerError},
    data_storage::DataStorageTemp,
    nodeids::NodeID,
    tasks::wait_ms,
};
use flmodules::{
    loopix::{
        broker::LoopixBroker,
        config::{LoopixConfig, LoopixRole},
        messages::LoopixMessage,
        storage::LoopixStorage,
    },
    network::messages::{NetworkIn, NetworkMessage, NetworkOut},
    nodeconfig::NodeConfig,
    overlay::{
        broker::direct::OverlayLoopix,
        messages::{NetworkWrapper, OverlayIn, OverlayMessage},
    },
    web_proxy::{
        broker::{WebProxy, WebProxyError},
        core::WebProxyConfig,
    },
};
use serde::{Deserialize, Serialize};
use tokio::sync::{
    mpsc::UnboundedReceiver,
    oneshot::{channel, Sender},
};
use x25519_dalek::{PublicKey, StaticSecret};

/**
 * This test sets up a number of Loopix nodes: clients, mixers, and providers.
 * You can either let it use the proxies or send a simple message through Loopix.
 * Start with the simple message!
 *
 * Some comments about the Loopix-implementation:
 * - it is not clear to me how the NodeID as u32 maps to different clients, mixers,
 *  and providers. Also, this will fail in the current implementation, as it uses
 *  NodeIDs provided by the system which will not follow your numbering schema.
 *  So you need to change the `LoopixConfig` to allow for random NodeIDs.
 * - for the same reason, passing NodeID as u32 to `default_with_path_length`
 *  is a very bad idea, as the real system will use random NodeIDs.
 * - I realize now that the setup of the brokers is really confusing - so I take some
 *  blame for your `LoopixBroker::start`. And it also shows that you had trouble
 *  implementing the `LoopixTranslate`... What you should do is to remove the
 *  `overlay: Broker<OverlayMessage>` from the `start` method. This broker is provided
 *  by the `OverlayLoopix::start` method, which takes in the `Broker<LoopixMessage>`.
 *  Happy to discuss this asynchronously over slack...
 */
#[tokio::test]
async fn test_loopix() -> Result<(), Box<dyn Error>> {
    let mut loopix_setup = LoopixSetup::new(2).await?;
    let mut network = NetworkSimul::new();
    network.add_nodes(loopix_setup.clients.clone()).await?;
    network.add_nodes(loopix_setup.mixers.clone()).await?;
    network.add_nodes(loopix_setup.providers.clone()).await?;
    let stop = network.process_loop();

    // I wouldn't start with the proxy :)
    if false {
        let mut proxy_src = ProxyBroker::new(loopix_setup.clients[0].loopix.clone()).await?;
        let proxy_dst = ProxyBroker::new(loopix_setup.providers[0].loopix.clone()).await?;

        proxy_src.proxy.get("https://fledg.re").await?;
        println!("Ids for proxies: ${} / ${}", proxy_src.id, proxy_dst.id);
    }

    if true {
        // Send a message from a client node to a provider:
        let id_dst = loopix_setup.providers[0].config.info.get_id();
        loopix_setup.clients[0]
            .overlay
            .emit_msg(OverlayMessage::Input(OverlayIn::NetworkWrapperToNetwork(
                id_dst,
                NetworkWrapper::wrap_yaml(
                    "Test",
                    &TestMessage {
                        field: "secret message".into(),
                    },
                )?,
            )))?;
        // Do something to look if the message arrived
        assert!(false);
    }

    // Quit the tokio-thread
    stop.send(true).ok();
    Ok(())
}

/**
 * Create a LoopixNode with all the necessary configuration for one node.
 * It holds the different brokers which can be used from the outside.
 * `LoopixNode`s are created by `LoopixSetup` and held there.
 */
#[derive(Clone)]
struct LoopixNode {
    config: NodeConfig,
    net: Broker<NetworkMessage>,
    overlay: Broker<OverlayMessage>,
    // This one should not be used from the outside.
    loopix: Broker<LoopixMessage>,
}

impl LoopixNode {
    async fn new(loopix_cfg: LoopixConfig) -> Result<Self, BrokerError> {
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
struct LoopixSetup {
    node_public_keys: HashMap<NodeID, PublicKey>,
    node_key_pairs: HashMap<NodeID, (PublicKey, StaticSecret)>,
    path_length: u32,
    clients: Vec<LoopixNode>,
    mixers: Vec<LoopixNode>,
    providers: Vec<LoopixNode>,
}

impl LoopixSetup {
    async fn new(path_length: u32) -> Result<Self, BrokerError> {
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

    async fn get_nodes(
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

    async fn get_node(&self, node_id: u32, role: LoopixRole) -> Result<LoopixNode, BrokerError> {
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

        LoopixNode::new(config).await
    }
}

/**
 * A very simple network simulator which connects different `Broker<NetworkMessage>`s
 * together using a `process` method which will handle one message from each broker.
 */
struct NetworkSimul {
    nodes: HashMap<NodeID, (Broker<NetworkMessage>, UnboundedReceiver<NetworkMessage>)>,
}

impl NetworkSimul {
    fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    async fn add_nodes(&mut self, nodes: Vec<LoopixNode>) -> Result<(), BrokerError> {
        for mut node in nodes {
            self.nodes.insert(
                node.config.info.get_id(),
                (node.net.clone(), node.net.get_tap().await?.0),
            );
        }
        Ok(())
    }

    fn process(&mut self) -> Result<(), BrokerError> {
        let mut msgs = vec![];
        for (&id_tx, (_, rx)) in self.nodes.iter_mut() {
            if let Ok(msg) = rx.try_recv() {
                if let NetworkMessage::Input(NetworkIn::MessageToNode(id_rx, node_msg)) = msg {
                    msgs.push((id_tx, id_rx, node_msg));
                }
            }
        }

        for (id_tx, id_rx, msg) in msgs {
            if let Some((broker, _)) = self.nodes.get_mut(&id_rx) {
                // This is for debugging and can be removed
                println!("{id_tx}->{id_rx}: {msg}");
                broker.emit_msg(NetworkMessage::Output(NetworkOut::MessageFromNode(
                    id_tx, msg,
                )))?;
            }
        }

        Ok(())
    }

    fn process_loop(mut self) -> Sender<bool> {
        let (tx, mut rx) = channel::<bool>();
        tokio::spawn(async move {
            // Loop over all messages from the loopix-nodes and pass them between the nodes.
            loop {
                if let Err(e) = self.process() {
                    println!("Error while processing network: {e:?}");
                }
                if let Ok(_) = rx.try_recv() {
                    println!("Terminating loop");
                    return;
                }
                // Wait for 100 ms.
                wait_ms(100).await;
            }
        });
        tx
    }
}

/**
 * Random message to be sent from a client to a provider.
 */
#[derive(Debug, Serialize, Deserialize)]
struct TestMessage {
    field: String,
}

/**
 * Once the main Loopix part works, you can try the ProxyBroker.
 * `WebProxy` needs the `OverlayOut::NodeInfosConnected` to get
 * a list of potential nodes where it can send a request to.
 * If this message is never received, it will panic.
 */
struct ProxyBroker {
    id: NodeID,
    proxy: WebProxy,
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

