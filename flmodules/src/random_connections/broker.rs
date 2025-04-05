use flarch::{
    broker::{Broker, Translate},
    nodeids::{NodeID, NodeIDs, U256},
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::{
    network::broker::{BrokerNetwork, NetworkIn, NetworkOut, MODULE_NAME},
    nodeconfig::NodeInfo,
    random_connections::{
        core::RandomStorage,
        messages::{Messages, ModuleMessage},
    },
    router::messages::NetworkWrapper,
    timer::Timer,
};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RandomIn {
    NodeList(Vec<NodeInfo>),
    NodeFailure(NodeID),
    NodeConnected(NodeID),
    NodeDisconnected(NodeID),
    NodeCommFromNetwork(NodeID, ModuleMessage),
    NetworkWrapperToNetwork(NodeID, NetworkWrapper),
    Tick,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RandomOut {
    ConnectNode(NodeID),
    DisconnectNode(NodeID),
    NodeIDsConnected(NodeIDs),
    NodeInfosConnected(Vec<NodeInfo>),
    NodeCommToNetwork(NodeID, ModuleMessage),
    NetworkWrapperFromNetwork(NodeID, NetworkWrapper),
}

pub type BrokerRandom = Broker<RandomIn, RandomOut>;

pub struct RandomBroker {
    pub broker: BrokerRandom,
    pub storage: watch::Receiver<RandomStorage>,
}

impl RandomBroker {
    pub async fn start(
        id: U256,
        broker_net: BrokerNetwork,
        timer: &mut Timer,
    ) -> anyhow::Result<Self> {
        let (messages, storage) = Messages::new(id);
        let mut broker = Broker::new();
        broker.add_handler(Box::new(messages)).await?;

        timer.tick_second(broker.clone(), RandomIn::Tick).await?;
        broker
            .add_translator_link(
                broker_net,
                Box::new(Self::link_rnd_net),
                Self::link_net_rnd(id),
            )
            .await?;

        Ok(Self { storage, broker })
    }

    fn link_net_rnd(our_id: U256) -> Translate<NetworkOut, RandomIn> {
        Box::new(move |msg: NetworkOut| match msg {
            NetworkOut::MessageFromNode(id, msg_nw) => msg_nw
                .unwrap_yaml::<ModuleMessage>(MODULE_NAME)
                .map(|msg_mod| RandomIn::NodeCommFromNetwork(id, msg_mod)),
            NetworkOut::NodeListFromWS(list) => Some(RandomIn::NodeList(
                list.into_iter()
                    .filter(|ni| ni.get_id() != our_id)
                    .collect(),
            )),
            NetworkOut::Connected(id) => return Some(RandomIn::NodeConnected(id)),
            NetworkOut::Disconnected(id) => return Some(RandomIn::NodeDisconnected(id)),
            _ => None,
        })
    }

    fn link_rnd_net(msg: RandomOut) -> Option<NetworkIn> {
        match msg {
            RandomOut::ConnectNode(id) => return Some(NetworkIn::Connect(id)),
            RandomOut::DisconnectNode(id) => return Some(NetworkIn::Disconnect(id)),
            RandomOut::NodeCommToNetwork(id, msg) => NetworkWrapper::wrap_yaml(MODULE_NAME, &msg)
                .ok()
                .map(|msg_nw| NetworkIn::MessageToNode(id, msg_nw)),
            _ => None,
        }
    }
}
