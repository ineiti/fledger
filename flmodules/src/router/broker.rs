use flarch::{
    add_translator_direct, add_translator_link,
    broker::{Broker, SubsystemHandler},
    nodeids::U256,
    platform_async_trait,
};

use super::messages::{RouterIn, RouterOut};
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
                Box::new(Self::translate_rnd_rtr),
                Box::new(Self::translate_rtr_rnd),
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
        })
    }

    fn translate_rtr_rnd(msg: RouterIn) -> Option<RandomIn> {
        let RouterIn::NetworkWrapperToNetwork(id, module_message) = msg;
        Some(RandomIn::NetworkWrapperToNetwork(id, module_message))
    }
}

/**
 * Connects directly to the Network broker.
 * Because the Network broker only delivers 'Connected' and 'Disconnected'
 * messages, this implementation has to do some bookkeeping to send out
 * the correct messages.
 */
#[derive(Debug, Default)]
pub struct RouterNetwork {
    nodes_available: Vec<NodeInfo>,
    nodes_connected: Vec<U256>,
}

#[derive(Debug, Clone)]
enum InternIn {
    Network(NetworkOut),
    Router(RouterIn),
}

#[derive(Debug, Clone, PartialEq)]
enum InternOut {
    Network(NetworkIn),
    Router(RouterOut),
}

impl RouterNetwork {
    pub async fn start(network: BrokerNetwork) -> anyhow::Result<BrokerRouter> {
        let mut intern = Broker::new_with_handler(Box::new(RouterNetwork::default()))
            .await?
            .0;
        add_translator_link!(intern, network, InternIn::Network, InternOut::Network);
        let b = Broker::new();
        add_translator_direct!(intern, b.clone(), InternIn::Router, InternOut::Router);

        Ok(b)
    }

    fn msg_router(&mut self, msg: RouterIn) -> Vec<InternOut> {
        let RouterIn::NetworkWrapperToNetwork(node, net_wrap) = msg;
        vec![InternOut::Network(NetworkIn::MessageToNode(node, net_wrap))]
    }

    fn msg_net(&mut self, msg: NetworkOut) -> Vec<InternOut> {
        let mut ret = false;
        let mut out = vec![];
        // Keep track of available and connected / disconnected nodes.
        match msg {
            NetworkOut::Connected(id) => {
                if !self.nodes_connected.contains(&id) {
                    self.nodes_connected.push(id);
                    out.push(RouterOut::Connected(id));
                    ret = true;
                }
            }
            NetworkOut::Disconnected(id) => {
                if self.nodes_connected.contains(&id) {
                    self.nodes_connected.retain(|&other| other != id);
                    out.push(RouterOut::Disconnected(id));
                    ret = true;
                }
            }
            NetworkOut::NodeListFromWS(vec) => {
                self.nodes_available = vec;
                ret = true;
            }
            NetworkOut::MessageFromNode(id, msg_nw) => {
                if !self.nodes_connected.contains(&id) {
                    log::warn!("Got message from unconnected node {id}: {msg_nw:?}");
                    self.nodes_connected.push(id);
                    out.push(RouterOut::Connected(id));
                    ret = true;
                }
                out.push(RouterOut::NetworkWrapperFromNetwork(id, msg_nw))
            }
            NetworkOut::SystemConfig(conf) => out.push(RouterOut::SystemConfig(conf)),
            _ => {}
        }

        // If something changed, output all relevant messages _before_ the network messages,
        // so the NodeIDsConnected comes before the node messages.
        if ret {
            out = [
                vec![
                    RouterOut::NodeInfoAvailable(self.nodes_available.clone()),
                    RouterOut::NodeIDsConnected(self.nodes_connected.clone().into()),
                    RouterOut::NodeInfosConnected(
                        self.nodes_available
                            .iter()
                            .filter(|node| self.nodes_connected.contains(&node.get_id()))
                            .cloned()
                            .collect(),
                    ),
                ],
                out,
            ]
            .concat();
        }

        out.into_iter()
            .map(|msg| InternOut::Router(msg))
            .collect::<Vec<_>>()
    }
}

#[platform_async_trait()]
impl SubsystemHandler<InternIn, InternOut> for RouterNetwork {
    async fn messages(&mut self, msgs: Vec<InternIn>) -> Vec<InternOut> {
        msgs.into_iter()
            .flat_map(|msg| match msg {
                InternIn::Network(network_out) => self.msg_net(network_out),
                InternIn::Router(router_in) => self.msg_router(router_in),
            })
            .collect::<Vec<_>>()
    }
}

#[cfg(test)]
mod test {
    use flarch::{nodeids::NodeID, start_logging_filter_level};

    use crate::nodeconfig::NodeConfig;

    use super::*;

    fn check_msgs(
        msgs: Vec<InternOut>,
        available: &[NodeInfo],
        connected: &[NodeInfo],
        total: usize,
    ) {
        assert_eq!(total, msgs.len());
        assert_eq!(
            InternOut::Router(RouterOut::NodeInfoAvailable(available.to_vec())),
            msgs[0]
        );
        assert_eq!(
            InternOut::Router(RouterOut::NodeIDsConnected(
                connected
                    .iter()
                    .map(|info| info.get_id())
                    .collect::<Vec<NodeID>>()
                    .into()
            )),
            msgs[1]
        );
        assert_eq!(
            InternOut::Router(RouterOut::NodeInfosConnected(connected.to_vec())),
            msgs[2]
        );
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
            .messages(vec![InternIn::Network(NetworkOut::NodeListFromWS(vec![
                nodes[0].clone(),
                nodes[1].clone(),
            ]))
            .into()])
            .await;
        check_msgs(msgs, &nodes, &[], 3);

        // Connect first node 0, then node 1
        let msgs = od
            .messages(vec![InternIn::Network(NetworkOut::Connected(
                nodes[0].get_id(),
            ))
            .into()])
            .await;
        check_msgs(msgs, &nodes, &[nodes[0].clone()], 4);

        let msgs = od
            .messages(vec![InternIn::Network(NetworkOut::Connected(
                nodes[1].get_id(),
            ))
            .into()])
            .await;
        check_msgs(msgs, &nodes, &nodes, 4);

        // Re-connect node 1, should do nothing
        let msgs = od
            .messages(vec![InternIn::Network(NetworkOut::Connected(
                nodes[1].get_id(),
            ))
            .into()])
            .await;
        assert_eq!(0, msgs.len());

        // Disconnect an unknown node - twice, in case it keeps it and would fail to remove it when
        // it isn't here anymore.
        let msgs = od
            .messages(vec![InternIn::Network(
                NetworkOut::Disconnected(node_unknown).into(),
            )])
            .await;
        assert_eq!(0, msgs.len());

        let msgs = od
            .messages(vec![InternIn::Network(
                NetworkOut::Disconnected(node_unknown).into(),
            )])
            .await;
        assert_eq!(0, msgs.len());

        // Disconnect a node
        let msgs = od
            .messages(vec![InternIn::Network(
                NetworkOut::Disconnected(nodes[0].get_id()).into(),
            )])
            .await;
        check_msgs(msgs, &nodes, &[nodes[1].clone()], 4);

        // Disconnect an unconnected node
        let msgs = od
            .messages(vec![InternIn::Network(
                NetworkOut::Disconnected(nodes[0].get_id()).into(),
            )])
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

        // log::info!("Sending nodelist");
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
