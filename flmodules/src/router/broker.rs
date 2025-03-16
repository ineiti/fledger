use flarch::{
    broker::{Broker, SubsystemHandler},
    nodeids::U256,
    platform_async_trait,
};

use super::messages::{RouterIn, RouterInternal, RouterOut};
use crate::{
    network::broker::{BrokerNetwork, NetworkIn, NetworkOut},
    nodeconfig::NodeInfo,
    random_connections::broker::{BrokerRandom, RandomIn, RandomOut},
};

pub type BrokerRouter = Broker<RouterIn, RouterOut>;

pub struct RouterRandom {}

impl RouterRandom {
    pub async fn start(mut random: BrokerRandom) -> anyhow::Result<BrokerRouter> {
        let b = Broker::new();
        // Translate RandomOut to FromRouter, and ToRouter to RandomIn.
        // A module connected to the Broker<RouterMessage> will get translations of the
        // RandomOut messages, and can send messages to RandomIn using the Router.
        random
            .add_translator_direct(
                b.clone(),
                Box::new(Self::translate_rtr_rnd),
                Box::new(Self::translate_rnd_rtr),
            )
            .await?;
        Ok(b)
    }

    fn translate_rnd_rtr(msg: RandomOut) -> Option<RouterOut> {
        Some(match msg {
            RandomOut::NodeIDsConnected(node_ids) => RouterOut::NodeIDsConnected(node_ids),
            RandomOut::NodeInfosConnected(infos) => RouterOut::NodeInfosConnected(infos),
            RandomOut::NetworkWrapperFromNetwork(id, module_message) => {
                RouterOut::NetworkWrapperFromNetwork(id, module_message)
            }
            _ => return None,
        })
    }

    fn translate_rtr_rnd(msg: RouterIn) -> Option<RandomIn> {
        match msg {
            RouterIn::NetworkWrapperToNetwork(id, module_message) => {
                Some(RandomIn::NetworkWrapperToNetwork(id, module_message))
            }
            _ => None,
        }
    }
}

/**
 * Connects directly to the Network broker.
 * Because the Network broker only delivers 'Connected' and 'Disconnected'
 * messages, this implementation has to do some bookkeeping to send out
 * the correct messages.
 */
pub struct RouterNetwork {
    nodes_available: Vec<NodeInfo>,
    nodes_connected: Vec<U256>,
}

impl RouterNetwork {
    pub async fn start(mut network: BrokerNetwork) -> anyhow::Result<BrokerRouter> {
        let mut b = Broker::new();
        // Subsystem for handling Connected and Disconnected messages from the Network broker.
        b.add_handler(Box::new(RouterNetwork {
            nodes_available: vec![],
            nodes_connected: vec![],
        }))
        .await?;

        // Translate NetworkOut to FromRouter, and ToRouter to NetworkIn.
        // A module connected to the Broker<RouterMessage> will get translations of the
        // NetworkOut messages, and can send messages to NetworkIn using the Router.
        network
            .add_translator_o_ti(
                b.clone(),
                Box::new(|msg| match msg {
                    NetworkOut::MessageFromNode(id, msg_str) => {
                        Some(RouterInternal::MessageFromNode(id, msg_str).into())
                    }
                    NetworkOut::NodeListFromWS(vec) => Some(RouterInternal::Available(vec).into()),
                    NetworkOut::Connected(id) => Some(RouterInternal::Connected(id).into()),
                    NetworkOut::Disconnected(id) => Some(RouterInternal::Disconnected(id).into()),
                    NetworkOut::SystemConfig(conf) => {
                        Some(RouterInternal::SystemConfig(conf).into())
                    }
                    _ => None,
                }),
            )
            .await?;
        b.add_translator_i_ti(
            network,
            Box::new(|msg| match msg {
                RouterIn::NetworkWrapperToNetwork(id, module_message) => {
                    if let Ok(msg_str) = serde_yaml::to_string(&module_message) {
                        Some(NetworkIn::MessageToNode(id, msg_str))
                    } else {
                        None
                    }
                }
                _ => None,
            }),
        )
        .await?;
        Ok(b)
    }

    fn available_connected(&self, id: U256) -> (bool, bool) {
        (
            self.nodes_available
                .iter()
                .find(|info| info.get_id() == id)
                .is_some(),
            self.nodes_connected.contains(&id),
        )
    }
}

#[platform_async_trait()]
impl SubsystemHandler<RouterIn, RouterOut> for RouterNetwork {
    async fn messages(&mut self, msgs: Vec<RouterIn>) -> Vec<RouterOut> {
        let mut ret = false;
        let mut out = vec![];
        // Keep track of available and connected / disconnected nodes.
        for msg in msgs {
            if let RouterIn::Internal(internal) = msg {
                match internal {
                    RouterInternal::Connected(id) => {
                        if self.available_connected(id) == (true, false) {
                            self.nodes_connected.push(id);
                            ret = true;
                        }
                    }
                    RouterInternal::Disconnected(id) => {
                        if self.available_connected(id).1 == true {
                            self.nodes_connected.retain(|&other| other != id);
                            ret = true;
                        }
                    }
                    RouterInternal::Available(vec) => {
                        self.nodes_available = vec;
                        ret = true;
                    }
                    RouterInternal::MessageFromNode(id, msg_str) => {
                        serde_yaml::from_str(&msg_str).ok().map(|module_message| {
                            out.push(RouterOut::NetworkWrapperFromNetwork(id, module_message))
                        });
                    }
                    RouterInternal::SystemConfig(conf) => out.push(RouterOut::SystemConfig(conf)),
                }
            }
        }

        // If something changed, output all relevant messages.
        if ret {
            out.extend(vec![
                RouterOut::NodeInfoAvailable(self.nodes_available.clone()),
                RouterOut::NodeIDsConnected(self.nodes_connected.clone().into()),
                RouterOut::NodeInfosConnected(
                    self.nodes_available
                        .iter()
                        .filter(|node| self.nodes_connected.contains(&node.get_id()))
                        .cloned()
                        .collect(),
                ),
            ]);
        }

        out
    }
}

#[cfg(test)]
mod test {
    use flarch::{nodeids::NodeID, start_logging_filter_level};

    use crate::nodeconfig::NodeConfig;

    use super::*;

    fn check_msgs(msgs: Vec<RouterOut>, available: &[NodeInfo], connected: &[NodeInfo]) {
        assert_eq!(3, msgs.len());
        assert_eq!(RouterOut::NodeInfoAvailable(available.to_vec()), msgs[0]);
        assert_eq!(
            RouterOut::NodeIDsConnected(
                connected
                    .iter()
                    .map(|info| info.get_id())
                    .collect::<Vec<NodeID>>()
                    .into()
            ),
            msgs[1]
        );
        assert_eq!(RouterOut::NodeInfosConnected(connected.to_vec()), msgs[2]);
    }

    #[tokio::test]
    async fn test_direct_dis_connect() -> anyhow::Result<()> {
        let mut od = RouterNetwork {
            nodes_available: vec![],
            nodes_connected: vec![],
        };
        let nodes = [NodeConfig::new().info, NodeConfig::new().info];
        let node_unknown = NodeConfig::new().info.get_id();

        // Start with two new nodes, but not yet connected.
        let msgs = od
            .messages(vec![RouterInternal::Available(vec![
                nodes[0].clone(),
                nodes[1].clone(),
            ])
            .into()])
            .await;
        check_msgs(msgs, &nodes, &[]);

        // Connect first node 0, then node 1
        let msgs = od
            .messages(vec![RouterInternal::Connected(nodes[0].get_id()).into()])
            .await;
        check_msgs(msgs, &nodes, &[nodes[0].clone()]);

        let msgs = od
            .messages(vec![RouterInternal::Connected(nodes[1].get_id()).into()])
            .await;
        check_msgs(msgs, &nodes, &nodes);

        // Re-connect node 1, should do nothing
        let msgs = od
            .messages(vec![RouterInternal::Connected(nodes[1].get_id()).into()])
            .await;
        assert_eq!(0, msgs.len());

        // Connect an unknown node - this should be ignored
        let msgs = od
            .messages(vec![RouterInternal::Connected(node_unknown).into()])
            .await;
        assert_eq!(0, msgs.len());

        // Disconnect an unknown node - twice, in case it keeps it and would fail to remove it when
        // it isn't here anymore.
        let msgs = od
            .messages(vec![RouterInternal::Disconnected(node_unknown).into()])
            .await;
        assert_eq!(0, msgs.len());

        let msgs = od
            .messages(vec![RouterInternal::Disconnected(node_unknown).into()])
            .await;
        assert_eq!(0, msgs.len());

        // Disconnect a node
        let msgs = od
            .messages(vec![RouterInternal::Disconnected(nodes[0].get_id()).into()])
            .await;
        check_msgs(msgs, &nodes, &[nodes[1].clone()]);

        // Disconnect an unconnected node
        let msgs = od
            .messages(vec![RouterInternal::Disconnected(nodes[0].get_id()).into()])
            .await;
        assert_eq!(0, msgs.len());

        Ok(())
    }

    #[tokio::test]
    async fn test_io() -> anyhow::Result<()> {
        start_logging_filter_level(vec![], log::LevelFilter::Info);

        let mut net = Broker::new();
        let mut rd = RouterNetwork::start(net.clone()).await?;
        net.settle(vec![]).await?;
        let mut tap_out = rd.get_tap_out().await?.0;

        log::info!("Sending nodelist");
        let nis = vec![NodeConfig::new().info];
        net.emit_msg_out(NetworkOut::NodeListFromWS(nis))?;
        net.settle(vec![]).await?;
        log::warn!("{}", tap_out.len());
        assert!(tap_out.len() > 0);
        assert!(matches!(
            tap_out.try_recv().unwrap(),
            RouterOut::NodeInfoAvailable(_)
        ));
        Ok(())
    }
}
