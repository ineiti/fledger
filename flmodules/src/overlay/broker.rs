use flarch::{
    broker::{Broker, BrokerError, Subsystem, SubsystemHandler},
    nodeids::U256,
    platform_async_trait,
};

use super::messages::{OverlayIn, OverlayInternal, OverlayMessage, OverlayOut};
use crate::{
    loopix::messages::{LoopixIn, LoopixMessage, LoopixOut},
    network::messages::{NetworkIn, NetworkMessage, NetworkOut},
    nodeconfig::NodeInfo,
    random_connections::messages::{RandomIn, RandomMessage, RandomOut},
};

pub struct OverlayRandom {}

impl OverlayRandom {
    pub async fn start(
        random: Broker<RandomMessage>,
    ) -> Result<Broker<OverlayMessage>, BrokerError> {
        let mut b = Broker::new();
        // Translate RandomOut to OverlayOut, and OverlayIn to RandomIn.
        // A module connected to the Broker<OverlayMessage> will get translations of the
        // RandomOut messages, and can send messages to RandomIn using the Overlay.
        b.link_bi(
            random,
            Box::new(|msg: RandomMessage| {
                if let RandomMessage::Output(out) = msg {
                    let ret = match out {
                        RandomOut::NodeIDsConnected(node_ids) => {
                            OverlayOut::NodeIDsConnected(node_ids)
                        }
                        RandomOut::NodeInfosConnected(infos) => {
                            OverlayOut::NodeInfosConnected(infos)
                        }
                        RandomOut::NetworkWrapperFromNetwork(id, module_message) => {
                            OverlayOut::NetworkWrapperFromNetwork(id, module_message)
                        }
                        _ => return None,
                    };
                    return Some(ret.into());
                }
                None
            }),
            Box::new(|msg| {
                if let OverlayMessage::Input(input) = msg {
                    let ret = match input {
                        OverlayIn::NetworkWrapperToNetwork(id, module_message) => {
                            RandomIn::NetworkWrapperToNetwork(id, module_message)
                        }
                    };
                    return Some(RandomMessage::Input(ret));
                }
                None
            }),
        )
        .await?;
        Ok(b)
    }
}

/**
 * Connects directly to the Network broker.
 * Because the Network broker only delivers 'Connected' and 'Disconnected'
 * messages, this implementation has to do some bookkeeping to send out
 * the correct messages.
 */
pub struct OverlayDirect {
    nodes_available: Vec<NodeInfo>,
    nodes_connected: Vec<U256>,
}

impl OverlayDirect {
    pub async fn start(
        direct: Broker<NetworkMessage>,
    ) -> Result<Broker<OverlayMessage>, BrokerError> {
        let mut b = Broker::new();
        // Subsystem for handling Connected and Disconnected messages from the Network broker.
        b.add_subsystem(Subsystem::Handler(Box::new(OverlayDirect {
            nodes_available: vec![],
            nodes_connected: vec![],
        })))
        .await?;

        // Translate NetworkOut to OverlayOut, and OverlayIn to NetworkIn.
        // A module connected to the Broker<OverlayMessage> will get translations of the
        // NetworkOut messages, and can send messages to NetworkIn using the Overlay.
        b.link_bi(
            direct,
            Box::new(|msg| {
                if let NetworkMessage::Output(out) = msg {
                    return match out {
                        NetworkOut::MessageFromNode(id, msg_str) => {
                            serde_yaml::from_str(&msg_str).ok().map(|module_message| {
                                OverlayOut::NetworkWrapperFromNetwork(id, module_message).into()
                            })
                        }
                        NetworkOut::NodeListFromWS(vec) => {
                            Some(OverlayInternal::Available(vec).into())
                        }
                        NetworkOut::Connected(id) => Some(OverlayInternal::Connected(id).into()),
                        NetworkOut::Disconnected(id) => {
                            Some(OverlayInternal::Disconnected(id).into())
                        }
                        _ => None,
                    };
                }
                None
            }),
            Box::new(|msg| {
                if let OverlayMessage::Input(input) = msg {
                    let ret = match input {
                        OverlayIn::NetworkWrapperToNetwork(id, module_message) => {
                            if let Ok(msg_str) = serde_yaml::to_string(&module_message) {
                                NetworkIn::MessageToNode(id, msg_str)
                            } else {
                                return None;
                            }
                        }
                    };
                    return Some(NetworkMessage::Input(ret));
                }
                None
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
impl SubsystemHandler<OverlayMessage> for OverlayDirect {
    async fn messages(&mut self, msgs: Vec<OverlayMessage>) -> Vec<OverlayMessage> {
        let mut ret = false;
        // Keep track of available and connected / disconnected nodes.
        for msg in msgs {
            if let OverlayMessage::Internal(internal) = msg {
                match internal {
                    OverlayInternal::Connected(id) => {
                        if self.available_connected(id) == (true, false) {
                            self.nodes_connected.push(id);
                            ret = true;
                        }
                    }
                    OverlayInternal::Disconnected(id) => {
                        if self.available_connected(id).1 == true {
                            self.nodes_connected.retain(|&other| other != id);
                            ret = true;
                        }
                    }
                    OverlayInternal::Available(vec) => {
                        self.nodes_available = vec;
                        ret = true;
                    }
                }
            }
        }

        // If something changed, output all relevant messages.
        if ret {
            vec![
                OverlayOut::NodeInfoAvailable(self.nodes_available.clone()),
                OverlayOut::NodeIDsConnected(self.nodes_connected.clone().into()),
                OverlayOut::NodeInfosConnected(
                    self.nodes_available
                        .iter()
                        .filter(|node| self.nodes_connected.contains(&node.get_id()))
                        .cloned()
                        .collect(),
                ),
            ]
            .into_iter()
            .map(|msg| msg.into())
            .collect()
        } else {
            vec![]
        }
    }
}

pub struct OverlayLoopix {
    pub broker: Broker<OverlayMessage>,
}

impl OverlayLoopix {
    pub async fn start(
        loopix: Broker<LoopixMessage>,
    ) -> Result<Broker<OverlayMessage>, BrokerError> {
        let mut broker = Broker::new();

        broker
            .link_bi(
                loopix.clone(),
                Box::new(Self::from_loopix),
                Box::new(Self::to_loopix),
            )
            .await?;

        Ok(broker)
    }

    fn from_loopix(msg: LoopixMessage) -> Option<OverlayMessage> {
        if let LoopixMessage::Output(out) = msg {
            let ret = match out {
                LoopixOut::NodeIDsConnected(node_ids) => OverlayOut::NodeIDsConnected(node_ids),
                LoopixOut::NodeInfosConnected(infos) => OverlayOut::NodeInfosConnected(infos),
                LoopixOut::OverlayReply(node_id, module_msg) => {
                    OverlayOut::NetworkWrapperFromNetwork(node_id, module_msg)
                }
                _ => return None,
            };
            return Some(ret.into());
        }
        None
    }

    fn to_loopix(msg: OverlayMessage) -> Option<LoopixMessage> {
        if let OverlayMessage::Input(input) = msg {
            match input {
                OverlayIn::NetworkWrapperToNetwork(node_id, wrapper) => {
                    return Some(LoopixIn::OverlayRequest(node_id, wrapper).into());
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod test {
    use flarch::nodeids::NodeID;

    use crate::nodeconfig::NodeConfig;

    use super::*;

    fn check_msgs(msgs: Vec<OverlayMessage>, available: &[NodeInfo], connected: &[NodeInfo]) {
        assert_eq!(3, msgs.len());
        assert_eq!(
            OverlayMessage::Output(OverlayOut::NodeInfoAvailable(available.to_vec())),
            msgs[0]
        );
        assert_eq!(
            OverlayMessage::Output(OverlayOut::NodeIDsConnected(
                connected
                    .iter()
                    .map(|info| info.get_id())
                    .collect::<Vec<NodeID>>()
                    .into()
            )),
            msgs[1]
        );
        assert_eq!(
            OverlayMessage::Output(OverlayOut::NodeInfosConnected(connected.to_vec())),
            msgs[2]
        );
    }

    #[tokio::test]
    async fn test_direct_dis_connect() -> Result<(), BrokerError> {
        let mut od = OverlayDirect {
            nodes_available: vec![],
            nodes_connected: vec![],
        };
        let nodes = [NodeConfig::new().info, NodeConfig::new().info];
        let node_unknown = NodeConfig::new().info.get_id();

        // Start with two new nodes, but not yet connected.
        let msgs = od
            .messages(vec![OverlayInternal::Available(vec![
                nodes[0].clone(),
                nodes[1].clone(),
            ])
            .into()])
            .await;
        check_msgs(msgs, &nodes, &[]);

        // Connect first node 0, then node 1
        let msgs = od
            .messages(vec![OverlayInternal::Connected(nodes[0].get_id()).into()])
            .await;
        check_msgs(msgs, &nodes, &[nodes[0].clone()]);

        let msgs = od
            .messages(vec![OverlayInternal::Connected(nodes[1].get_id()).into()])
            .await;
        check_msgs(msgs, &nodes, &nodes);

        // Re-connect node 1, should do nothing
        let msgs = od
            .messages(vec![OverlayInternal::Connected(nodes[1].get_id()).into()])
            .await;
        assert_eq!(0, msgs.len());

        // Connect an unknown node - this should be ignored
        let msgs = od
            .messages(vec![OverlayInternal::Connected(node_unknown).into()])
            .await;
        assert_eq!(0, msgs.len());

        // Disconnect an unknown node - twice, in case it keeps it and would fail to remove it when
        // it isn't here anymore.
        let msgs = od
            .messages(vec![OverlayInternal::Disconnected(node_unknown).into()])
            .await;
        assert_eq!(0, msgs.len());

        let msgs = od
            .messages(vec![OverlayInternal::Disconnected(node_unknown).into()])
            .await;
        assert_eq!(0, msgs.len());

        // Disconnect a node
        let msgs = od
            .messages(vec![OverlayInternal::Disconnected(nodes[0].get_id()).into()])
            .await;
        check_msgs(msgs, &nodes, &[nodes[1].clone()]);

        // Disconnect an unconnected node
        let msgs = od
            .messages(vec![OverlayInternal::Disconnected(nodes[0].get_id()).into()])
            .await;
        assert_eq!(0, msgs.len());

        Ok(())
    }
}
