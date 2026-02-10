use flarch::{
    broker::{Broker, SubsystemHandler},
    platform_async_trait,
};
use flmodules::{
    dht_router::broker::{DHTRouterIn, DHTRouterOut},
    dht_storage::broker::{DHTStorageIn, DHTStorageOut},
    network::{broker::NetworkOut, signal::FledgerConfig},
    nodeconfig::NodeInfo,
    timer::TimerMessage,
};
use flnode::node::Node;
use serde::{Deserialize, Serialize};

use crate::proxy::{
    broadcast::{BroadcastFromTabs, BroadcastToTabs},
    proxy::{ProxyIn, ProxyOut},
};

#[derive(Debug, Serialize, Deserialize)]
enum MsgToLeader {
    DHTStorage(DHTStorageIn),
    DHTRouter(DHTRouterIn),
}

#[derive(Debug, Serialize, Deserialize)]
enum MsgFromLeader {
    DHTStorage(DHTStorageOut),
    DHTRouter(DHTRouterOut),
    SystemConfig(FledgerConfig),
    NodeListFromWS(Vec<NodeInfo>),
}

#[derive(Debug, Clone)]
pub(super) enum InternIn {
    Proxy(ProxyIn),
    DHTStorage(DHTStorageIn),
    Timer(TimerMessage),
    Broadcast(BroadcastFromTabs),
}

#[derive(Debug, Clone)]
pub(super) enum InternOut {
    Proxy(ProxyOut),
    DHTStorage(DHTStorageOut),
    DHTRouter(DHTRouterOut),
    Network(NetworkOut),
    Broadcast(BroadcastToTabs),
}

type BrokerIntern = Broker<InternIn, InternOut>;

pub(super) struct Intern {
    id: TabID,
    tabs: Vec<TabID>,
    danu: Option<Node>,
}

impl Intern {
    pub(super) async fn start(id: TabID) -> anyhow::Result<BrokerIntern> {
        Ok(Broker::new_with_handler(Box::new(Intern {
            id,
            tabs: vec![id],
            danu: None,
        }))
        .await?
        .0)
    }

    fn is_leader(&self) -> bool {
        if let Some(id) = self.tabs.get(0) {
            return id == &self.id;
        }
        false
    }

    fn timer(&mut self) -> Vec<InternOut> {
        vec![InternOut::Broadcast(BroadcastToTabs::Alive)]
    }

    fn broadcast(&mut self, msg: BroadcastFromTabs) -> Vec<InternOut> {
        let mut out = vec![];
        match msg {
            BroadcastFromTabs::Alive(tab_id) => {
                if !self.tabs.contains(&tab_id) {
                    self.tabs.push(tab_id);
                    self.tabs.sort();
                    out.push(InternOut::Proxy(ProxyOut::TabList(self.tabs.clone())));
                }
            }
            BroadcastFromTabs::Stopped(tab_id) => {
                if self.tabs.contains(&tab_id) {
                    self.tabs.retain(|id| id != &tab_id);
                }
            }
            BroadcastFromTabs::ToLeader { from, data } => {
                if self.is_leader() {
                    log::trace!("Got message from {from:?}");
                    if let Ok(msg) = serde_json::from_str::<MsgToLeader>(&data) {
                        if let Some(danu) = self.danu.as_mut() {
                            if let Err(e) = match &msg {
                                MsgToLeader::DHTStorage(dhts) => danu
                                    .dht_storage
                                    .as_mut()
                                    .unwrap()
                                    .broker
                                    .emit_msg_in(dhts.clone()),
                                MsgToLeader::DHTRouter(dhtr) => danu
                                    .dht_router
                                    .as_mut()
                                    .unwrap()
                                    .broker
                                    .emit_msg_in(dhtr.clone()),
                            } {
                                log::error!("While passing {msg:?} to danu: {e:?}");
                            }
                        }
                    }
                }
            }
            BroadcastFromTabs::FromLeader(data) => {
                if !self.is_leader() {
                    if let Ok(msg) = serde_json::from_str::<MsgFromLeader>(&data) {
                        out.push(match msg {
                            MsgFromLeader::DHTStorage(dhts) => InternOut::DHTStorage(dhts),
                            MsgFromLeader::DHTRouter(dhtr) => InternOut::DHTRouter(dhtr),
                            MsgFromLeader::SystemConfig(fledger_config) => {
                                InternOut::Network(NetworkOut::SystemConfig(fledger_config))
                            }
                            MsgFromLeader::NodeListFromWS(node_infos) => {
                                InternOut::Network(NetworkOut::NodeListFromWS(node_infos))
                            }
                        });
                    }
                }
            }
        }
        out
    }
}

#[platform_async_trait]
impl SubsystemHandler<InternIn, InternOut> for Intern {
    async fn messages(&mut self, msgs: Vec<InternIn>) -> Vec<InternOut> {
        let mut out = vec![];
        for msg in msgs {
            match msg {
                InternIn::Proxy(_) => {}
                InternIn::DHTStorage(input) => {
                    if let Ok(s) = serde_json::to_string(&input) {
                        out.push(InternOut::Broadcast(BroadcastToTabs::ToLeader(s)));
                    }
                }
                InternIn::Timer(timer_message) => {
                    if timer_message == TimerMessage::Second {
                        out.extend(self.timer());
                    }
                }
                InternIn::Broadcast(broadcast_io) => out.extend(self.broadcast(broadcast_io)),
            }
        }
        out
    }
}

/// Unique identifier for a node, used for leader election.
/// Lower NodeID = higher priority for leadership.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabID {
    timestamp_secs: u64,
    random_component: u32,
}

impl PartialOrd for TabID {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TabID {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.timestamp_secs
            .cmp(&other.timestamp_secs)
            .then_with(|| self.random_component.cmp(&other.random_component))
    }
}

impl std::fmt::Display for TabID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.timestamp_secs, self.random_component)
    }
}

impl TabID {
    pub fn new() -> Self {
        let timestamp_secs = (js_sys::Date::now() / 1000.0) as u64;
        let mut buf = [0u8; 4];
        getrandom::getrandom(&mut buf).expect("getrandom failed");
        let random_component = u32::from_le_bytes(buf);
        Self {
            timestamp_secs,
            random_component,
        }
    }
}
