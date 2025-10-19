use std::collections::HashMap;

use anyhow::Result;
use flarch::{
    broker::{Broker, SubsystemHandler},
    data_storage::DataStorage,
    nodeids::NodeID,
    platform_async_trait,
};
use serde::{Deserialize, Serialize};

use crate::{
    network::broker::{NetworkIn, NetworkOut},
    nodeconfig::NodeInfo,
    router::messages::NetworkWrapper,
    template::persistent::broker::{PersistentIn, PersistentOut},
};

#[derive(Debug, Clone, Default)]
pub struct PersistentConfig {
    pub id: NodeID,
    pub reset: bool,
}

// The message sent by this module over the network.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(super) enum ModuleMessage {
    Counter(usize),
}

// The name of this module - must be unique to the danu system.
pub(super) const MODULE_NAME: &str = "TEMPALTE_PERSISTENT";

// All possible incoming messages wrapped by their source.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum InternIn {
    Timer,
    Persistent(PersistentIn),
    Network(NetworkOut),
}

// All possible output messages which will be automatically handled
// by the broker.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum InternOut {
    Persistent(PersistentOut),
    Network(NetworkIn),
}

// The internal state of this broker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct InternState {
    pub counter: usize,
    pub other: HashMap<NodeID, usize>,
    pub nodes: Vec<NodeInfo>,
}

// The handler for Intern messages.
#[derive(Debug)]
pub(super) struct Intern {
    cfg: PersistentConfig,
    ds: Box<dyn DataStorage + Send>,
    state: InternState,
}

pub(super) type BrokerIntern = Broker<InternIn, InternOut>;

impl Intern {
    pub(super) async fn broker(
        ds: Box<dyn DataStorage + Send>,
        cfg: PersistentConfig,
    ) -> Result<BrokerIntern> {
        let mut state = InternState::default();
        if let Ok(state_str) = ds.get(MODULE_NAME) {
            if let Ok(state_stored) = serde_yaml::from_str(&state_str) {
                state = state_stored;
            }
        }
        Ok(
            Broker::new_with_handler(Box::new(Intern { cfg, ds, state }))
                .await?
                .0,
        )
    }

    // If it's a PersistentIn::Count, increase the counter by 1, and update the
    // stats if the counter is even, also sending it to all available nodes.
    fn msg_pers(&mut self, msg: PersistentIn) -> Vec<InternOut> {
        match msg {
            PersistentIn::Count => {
                self.state.counter += 1;
                self.store_state();
                if self.state.counter % 2 == 0 {
                    return self.update_states();
                }
            }
        }
        vec![]
    }

    // Process the timer message: set the counter to 0, update the state, and request
    // an update of the available nodes.
    fn msg_timer(&mut self) -> Vec<InternOut> {
        log::info!("Reset is {}", self.cfg.reset);
        if self.cfg.reset {
            self.state.counter = 0;
        }
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
                    self.state.other.insert(from, update);
                    return self.update_states();
                }
            }
            NetworkOut::NodeListFromWS(nodes) => self.state.nodes = nodes,
            _ => {}
        }
        vec![]
    }

    fn update_states(&self) -> Vec<InternOut> {
        let mut out = vec![InternOut::Persistent(PersistentOut::State(
            self.state.clone(),
        ))];
        if let Ok(net_msg) =
            NetworkWrapper::wrap_yaml(MODULE_NAME, &ModuleMessage::Counter(self.state.counter))
        {
            for node in &self.state.nodes {
                if node.get_id() != self.cfg.id {
                    out.push(InternOut::Network(NetworkIn::MessageToNode(
                        node.get_id(),
                        net_msg.clone(),
                    )));
                }
            }
        }
        log::debug!("Returning states: {:?}", out);
        out
    }

    fn store_state(&mut self) {
        let mut state = self.state.clone();
        state.nodes.clear();
        if let Err(e) = self
            .ds
            .set(MODULE_NAME, &serde_yaml::to_string(&state).unwrap())
        {
            log::warn!("Couldn't update data: {e:?}");
        }
    }
}

#[platform_async_trait()]
impl SubsystemHandler<InternIn, InternOut> for Intern {
    // The message handler necessary for implementing a broker.
    async fn messages(&mut self, msgs: Vec<InternIn>) -> Vec<InternOut> {
        let out = msgs
            .into_iter()
            .inspect(|msg| log::debug!("{msg:?}"))
            .flat_map(|msg| match msg {
                InternIn::Timer => self.msg_timer(),
                InternIn::Persistent(pers_in) => self.msg_pers(pers_in),
                InternIn::Network(network_out) => self.msg_net(network_out),
            })
            .inspect(|msg| log::debug!("output: {msg:?}"))
            .collect();
        log::debug!("Output is: {out:?}");
        out
    }
}
