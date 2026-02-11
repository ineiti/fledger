use flarch::{
    broker::{Broker, SubsystemHandler},
    data_storage::DataStorageLocal,
    platform_async_trait,
    web_rtc::connection::{ConnectionConfig, HostLogin},
};
use flmodules::{
    dht_router::broker::{DHTRouterIn, DHTRouterOut},
    dht_storage::broker::{DHTStorageIn, DHTStorageOut},
    network::{broker::NetworkOut, signal::FledgerConfig},
    nodeconfig::NodeInfo,
    timer::TimerMessage,
    Modules,
};
use flnode::node::Node;
use serde::{Deserialize, Serialize};

use crate::{
    danode::NetConf,
    proxy::{
        broadcast::{BroadcastFromTabs, BroadcastToTabs},
        proxy::{ProxyIn, ProxyOut},
    },
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

#[derive(Debug, Clone, PartialEq)]
pub(super) enum InternIn {
    // To minimize the possibility of having multiple tabs opening at
    // the same time, and all deciding to become leaders, the proxy
    // must send three Start messages 100ms apart. Only after the third
    // Start message is received will Intern decide whether it is the
    // leader or not.
    Start,
    Proxy(ProxyIn),
    DHTStorage(DHTStorageIn),
    Timer(TimerMessage),
    Broadcast(BroadcastFromTabs),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum InternOut {
    Proxy(ProxyOut),
    DHTStorage(DHTStorageOut),
    DHTRouter(DHTRouterOut),
    Network(NetworkOut),
    Broadcast(BroadcastToTabs),
}

pub(super) type BrokerIntern = Broker<InternIn, InternOut>;

pub(super) struct Intern {
    broker: BrokerIntern,
    cfg: NetConf,
    id: TabID,
    tabs: Vec<TabID>,
    danu: Option<Node>,
    start: u32,
}

impl Intern {
    pub(super) async fn start(cfg: NetConf, id: TabID) -> anyhow::Result<BrokerIntern> {
        let mut broker = Broker::new();
        broker
            .add_handler(Box::new(Intern {
                cfg,
                id,
                tabs: vec![id],
                danu: None,
                start: 3,
                broker: broker.clone(),
            }))
            .await?;

        Ok(broker)
    }

    fn is_leader(&self) -> bool {
        if let Some(id) = self.tabs.first() {
            return id == &self.id;
        }
        false
    }

    async fn start_danu(&mut self) -> anyhow::Result<()> {
        let my_storage = DataStorageLocal::new(&self.cfg.storage_name);
        let mut node_config = Node::get_config(my_storage.clone())?;
        let config = ConnectionConfig::new(
            self.cfg.signal_server.clone(),
            self.cfg
                .stun_server
                .as_ref()
                .and_then(|url| Some(HostLogin::from_url(&url.clone()))),
            self.cfg
                .turn_server
                .as_ref()
                .and_then(|url| HostLogin::from_login_url(&url).ok()),
        );
        node_config.info.modules = Modules::stable() - Modules::WEBPROXY_REQUESTS;
        let mut node = Node::start_network(my_storage, node_config, config).await?;
        node.dht_router
            .as_mut()
            .unwrap()
            .broker
            .add_translator_o_to(
                self.broker.clone(),
                Box::new(|msg| Some(InternOut::DHTRouter(msg))),
            )
            .await?;
        node.dht_storage
            .as_mut()
            .unwrap()
            .broker
            .add_translator_o_to(
                self.broker.clone(),
                Box::new(|msg| Some(InternOut::DHTStorage(msg))),
            )
            .await?;
        self.danu = Some(node);
        Ok(())
    }

    async fn msg_start(&mut self) -> Vec<InternOut> {
        if self.start > 0 {
            self.start -= 1;
            return vec![InternOut::Broadcast(BroadcastToTabs::Alive)];
        }
        if self.is_leader() {
            if let Err(e) = self.start_danu().await {
                log::error!("Couldn't start danu: {e:?}");
            } else {
                return vec![InternOut::Proxy(ProxyOut::Elected)];
            }
        }
        vec![InternOut::Broadcast(BroadcastToTabs::Alive)]
    }

    fn msg_timer(&mut self) -> Vec<InternOut> {
        vec![InternOut::Broadcast(BroadcastToTabs::Alive)]
    }

    fn msg_broadcast(&mut self, msg: BroadcastFromTabs) -> Vec<InternOut> {
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
                InternIn::Start => out.extend(self.msg_start().await),
                InternIn::Proxy(_) => {}
                InternIn::DHTStorage(input) => {
                    if let Ok(s) = serde_json::to_string(&input) {
                        out.push(InternOut::Broadcast(BroadcastToTabs::ToLeader(s)));
                    }
                }
                InternIn::Timer(timer_message) => {
                    if timer_message == TimerMessage::Second {
                        out.extend(self.msg_timer());
                    }
                }
                InternIn::Broadcast(broadcast_io) => out.extend(self.msg_broadcast(broadcast_io)),
            }
        }
        out
    }
}

/// Unique identifier for a node, used for leader election.
/// Lower NodeID = higher priority for leadership.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
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

    #[cfg(test)]
    pub fn new_const(timestamp_secs: u64, random_component: u32) -> Self {
        Self {
            timestamp_secs,
            random_component,
        }
    }
}

#[cfg(test)]
mod test {
    use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

    use crate::proxy::broadcast::test::Tab;
    wasm_bindgen_test_configure!(run_in_browser);

    use super::*;
    #[wasm_bindgen_test(async)]
    async fn is_leader() -> anyhow::Result<()> {
        wasm_logger::init(wasm_logger::Config::new(log::Level::Debug));

        let tab0 = Tab::new_const(TabID::new_const(0, 0)).await?;
        let tab1 = Tab::new_const(TabID::new_const(0, 1)).await?;
        let cfg = NetConf::default();
        let mut int0 = Intern::start(cfg.clone(), tab0.id.clone()).await?;
        let mut int1 = Intern::start(cfg, tab1.id.clone()).await?;
        let mut itap0 = int0.get_tap_out().await?.0;
        let mut itap1 = int1.get_tap_out().await?.0;

        for i in 0..3 {
            log::info!("Step {i}");
            int0.emit_msg_in(InternIn::Start)?;
            let msg = itap0.recv().await.unwrap();
            assert_eq!(InternOut::Broadcast(BroadcastToTabs::Alive), msg);
            int1.emit_msg_in(InternIn::Broadcast(BroadcastFromTabs::Alive(tab0.id)))?;
            if i == 0 {
                assert_eq!(
                    InternOut::Proxy(ProxyOut::TabList(vec![tab0.id.clone(), tab1.id.clone()])),
                    itap1.recv().await.unwrap()
                );
            }

            int1.emit_msg_in(InternIn::Start)?;
            let msg = itap1.recv().await.unwrap();
            assert_eq!(InternOut::Broadcast(BroadcastToTabs::Alive), msg);
            int0.emit_msg_in(InternIn::Broadcast(BroadcastFromTabs::Alive(tab1.id)))?;
            if i == 0 {
                assert_eq!(
                    InternOut::Proxy(ProxyOut::TabList(vec![tab0.id.clone(), tab1.id.clone()])),
                    itap0.recv().await.unwrap()
                );
            }
        }

        log::info!("Sending last start to start danu");
        int0.emit_msg_in(InternIn::Start)?;
        let msg = itap0.recv().await.unwrap();
        assert_eq!(InternOut::Proxy(ProxyOut::Elected), msg);

        log::info!("Sending start to int1 which should not start danu");
        int1.emit_msg_in(InternIn::Start)?;
        let msg = itap1.recv().await.unwrap();
        assert_eq!(InternOut::Broadcast(BroadcastToTabs::Alive), msg);

        Ok(())
    }
}
