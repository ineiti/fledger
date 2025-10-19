use std::collections::HashMap;

use anyhow::Result;
use flarch::{
    broker::{Broker, SubsystemHandler},
    nodeids::NodeID,
    platform_async_trait,
};
use serde::{Deserialize, Serialize};

use crate::{
    network::broker::{NetworkIn, NetworkOut},
    nodeconfig::NodeInfo,
    router::messages::NetworkWrapper,
    template::multi::broker::{MultiIn, MultiOut},
};

// The message sent by this module over the network.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) enum ModuleMessage {
    Counter(usize),
}

// The name of this module - must be unique to the danu system.
pub(crate) const MODULE_NAME: &str = "TEMPALTE_MULTI";

// All possible incoming messages wrapped by their source.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum InternIn {
    Timer,
    Multi(MultiIn),
    Network(NetworkOut),
}

// All possible output messages which will be automatically handled
// by the broker.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum InternOut {
    Multi(MultiOut),
    Network(NetworkIn),
}

// The internal state of this broker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Message {
    pub counter: usize,
    pub other: HashMap<NodeID, usize>,
    pub(crate) nodes: Vec<NodeInfo>,
}

pub(crate) type BrokerIntern = Broker<InternIn, InternOut>;

impl Message {
    pub(crate) async fn broker() -> Result<BrokerIntern> {
        Ok(Broker::new_with_handler(Box::new(Message::default()))
            .await?
            .0)
    }

    // If it's a MultiIn::Count, increase the counter by 1, and update the
    // stats if the counter is even, also sending it to all available nodes.
    fn msg_multi(&mut self, msg: MultiIn) -> Vec<InternOut> {
        match msg {
            MultiIn::Count => {
                self.counter += 1;
                log::debug!("Count is: {}", self.counter);
                if self.counter % 2 == 0 {
                    return self.update_states();
                }
            }
        }
        vec![]
    }

    // Process the timer message: set the counter to 0, update the state, and request
    // an update of the available nodes.
    fn msg_timer(&mut self) -> Vec<InternOut> {
        self.counter = 0;
        [
            vec![InternOut::Network(NetworkIn::WSUpdateListRequest)],
            self.update_states(),
        ]
        .concat()
    }

    // Handle network messages
    fn msg_net(&mut self, msg: NetworkOut) -> Vec<InternOut> {
        match msg {
            NetworkOut::MessageFromNode(from, net) => {
                if let Some(ModuleMessage::Counter(update)) = net.unwrap_yaml(MODULE_NAME) {
                    self.other.insert(from, update);
                    return self.update_states();
                }
            }
            NetworkOut::NodeListFromWS(nodes) => self.nodes = nodes,
            _ => {}
        }
        vec![]
    }

    fn update_states(&self) -> Vec<InternOut> {
        let mut out = vec![InternOut::Multi(MultiOut::State(self.clone()))];
        if let Ok(net_msg) =
            NetworkWrapper::wrap_yaml(MODULE_NAME, &ModuleMessage::Counter(self.counter))
        {
            for node in &self.nodes {
                out.push(InternOut::Network(NetworkIn::MessageToNode(
                    node.get_id(),
                    net_msg.clone(),
                )));
            }
        }
        log::debug!("Returning states: {:?}", out);
        out
    }
}

#[platform_async_trait()]
impl SubsystemHandler<InternIn, InternOut> for Message {
    // The message handler necessary for implementing a broker.
    async fn messages(&mut self, msgs: Vec<InternIn>) -> Vec<InternOut> {
        let out = msgs.into_iter()
            .inspect(|msg| log::debug!("{msg:?}"))
            .flat_map(|msg| match msg {
                InternIn::Timer => self.msg_timer(),
                InternIn::Multi(multi_in) => self.msg_multi(multi_in),
                InternIn::Network(network_out) => self.msg_net(network_out),
            })
            .inspect(|msg| log::debug!("output: {msg:?}"))
            .collect();
        log::debug!("Output is: {out:?}");
        out
    }
}
