use std::collections::HashMap;

use flarch::{
    broker::{Broker, SubsystemHandler},
    data_storage::DataStorageLocal,
    nodeids::NodeID,
    platform_async_trait,
    web_rtc::connection::{ConnectionConfig, HostLogin},
};
use flmodules::{
    dht_router::broker::{BrokerDHTRouter, DHTRouterOut},
    dht_storage::broker::{BrokerDHTStorage, DHTStorageIn, DHTStorageOut},
    flo::realm::RealmID,
    network::{
        broker::{BrokerNetwork, NetworkOut},
        signal::FledgerConfig,
    },
    nodeconfig::NodeInfo,
    timer::TimerMessage,
    Modules,
};
use flnode::node::Node;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::wasm_bindgen;

use crate::{
    danode::NetConf,
    proxy::{
        broadcast::{BroadcastFromTabs, BroadcastToTabs, MsgFromLeader, MsgToLeader},
        proxy::{ProxyIn, ProxyOut},
    },
};

#[derive(Debug, Clone, PartialEq)]
pub(super) enum InternIn {
    // To minimize the possibility of having multiple tabs opening at
    // the same time, and all deciding to become leaders, the proxy
    // must send three Start messages 100ms apart. Only after the third
    // Start message is received will Intern decide whether it is the
    // leader or not.
    Start,
    Proxy(ProxyIn),
    Timer(TimerMessage),
    Broadcast(BroadcastFromTabs),
    // This is used so Intern.is_leader()==true can cache some of the
    // messages, and when a new tab joins, the leader can directly
    // send the latest values.
    FromNode(MsgFromLeader),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum InternOut {
    Proxy(ProxyOut),
    Broadcast(BroadcastToTabs),
}

pub(super) type BrokerIntern = Broker<InternIn, InternOut>;

pub(super) struct Intern {
    broker: BrokerIntern,
    cfg: NetConf,
    id: TabID,
    alive: HashMap<TabID, usize>,
    danu: Option<Node>,
    start: u32,
    network: BrokerNetwork,
    dht_router: BrokerDHTRouter,
    dht_storage: BrokerDHTStorage,
    cache: Cache,
}

impl Intern {
    pub(super) async fn start(
        cfg: NetConf,
        id: TabID,
        network: BrokerNetwork,
        dht_router: BrokerDHTRouter,
        dht_storage: BrokerDHTStorage,
    ) -> anyhow::Result<BrokerIntern> {
        let mut broker = Broker::new();
        let mut intern = Intern {
            cfg,
            id,
            danu: None,
            start: 3,
            broker: broker.clone(),
            alive: HashMap::new(),
            network,
            dht_router,
            dht_storage,
            cache: Cache::default(),
        };
        intern.add_translators().await?;
        broker.add_handler(Box::new(intern)).await?;

        Ok(broker)
    }

    async fn add_translators(&mut self) -> anyhow::Result<()> {
        self.network
            .add_translator_o_ti(
                self.broker.clone(),
                Box::new(|m| match m {
                    NetworkOut::NodeListFromWS(node_infos) => Some(InternIn::FromNode(
                        MsgFromLeader::NodeListFromWS(node_infos),
                    )),
                    NetworkOut::SystemConfig(fledger_config) => Some(InternIn::FromNode(
                        MsgFromLeader::SystemConfig(fledger_config),
                    )),
                    _ => None,
                }),
            )
            .await?;
        self.dht_router
            .add_translator_o_ti(
                self.broker.clone(),
                Box::new(|m| Some(InternIn::FromNode(MsgFromLeader::DHTRouter(m)))),
            )
            .await?;
        self.dht_storage
            .add_translator_o_ti(
                self.broker.clone(),
                Box::new(|m| Some(InternIn::FromNode(MsgFromLeader::DHTStorage(m)))),
            )
            .await?;
        Ok(())
    }

    fn tab_ids_sorted(&self) -> Vec<TabID> {
        let mut sorted = self
            .alive
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        sorted.sort();
        sorted
    }

    fn is_leader(&self) -> bool {
        if let Some(id) = self.tab_ids_sorted().first() {
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
        node.broker_net.link_direct(self.network.clone()).await?;
        node.dht_router
            .as_mut()
            .unwrap()
            .broker
            .link_direct(self.dht_router.clone())
            .await?;
        let ds = &mut node.dht_storage.as_mut().unwrap().broker;
        ds.link_direct(self.dht_storage.clone()).await?;
        ds.emit_msg_in(DHTStorageIn::GetRealms)?;
        self.danu = Some(node);
        Ok(())
    }

    async fn msg_start(&mut self) -> Vec<InternOut> {
        if self.start > 0 {
            self.start -= 1;
            return vec![InternOut::Broadcast(BroadcastToTabs::Alive(None))];
        }
        if self.is_leader() {
            if let Err(e) = self.start_danu().await {
                log::error!("Couldn't start danu: {e:?}");
            }
            return vec![InternOut::Proxy(ProxyOut::Elected)];
        }
        if let Err(e) = self.setup_client_translations().await {
            log::error!("Couldn't setup client translations: {e:?}");
        }
        vec![InternOut::Broadcast(BroadcastToTabs::ToLeader(
            MsgToLeader::GetUpdate,
        ))]
    }

    async fn setup_client_translations(&mut self) -> anyhow::Result<()> {
        self.dht_router
            .add_translator_i_to(
                self.broker.clone(),
                Box::new(|m| {
                    Some(InternOut::Broadcast(BroadcastToTabs::ToLeader(
                        MsgToLeader::DHTRouter(m),
                    )))
                }),
            )
            .await?;
        self.dht_storage
            .add_translator_i_to(
                self.broker.clone(),
                Box::new(|m| {
                    Some(InternOut::Broadcast(BroadcastToTabs::ToLeader(
                        MsgToLeader::DHTStorage(m),
                    )))
                }),
            )
            .await?;
        Ok(())
    }

    async fn msg_timer(&mut self) -> Vec<InternOut> {
        if self.start > 0 {
            return vec![];
        }

        self.alive.insert(self.id.clone(), 5);
        // log::info!(
        //     "msg_timer: {}",
        //     self.tab_ids_sorted()
        //         .iter()
        //         .map(|t| format!("{t}/{}", self.alive.get(t).unwrap()))
        //         .collect::<Vec<_>>()
        //         .join(" : ")
        // );
        for kv in &mut self.alive {
            if *kv.1 > 0 {
                *kv.1 -= 1;
            }
        }
        self.alive.retain(|_, v| *v > 0);

        let mut out = vec![];
        if self.is_leader() && self.danu.is_none() {
            if let Err(e) = self.start_danu().await {
                log::error!("Couldn't start danu: {e:?}");
            }
            out.push(InternOut::Proxy(ProxyOut::Elected));
        }
        out.push(InternOut::Broadcast(BroadcastToTabs::Alive(Some(
            self.is_leader(),
        ))));

        out
    }

    async fn msg_broadcast(&mut self, msg: BroadcastFromTabs) -> Vec<InternOut> {
        let mut out = vec![];
        match msg {
            BroadcastFromTabs::Alive { from, is_leader } => {
                let new_tab = !self.alive.contains_key(&from);
                self.alive.insert(from, 5);
                let tabs = self.tab_ids_sorted();
                if new_tab {
                    out.push(InternOut::Proxy(ProxyOut::TabList(tabs.clone())));
                }
                if tabs.first().unwrap() == &from && is_leader == Some(false) && self.start == 0 {
                    log::error!("No leader present");
                }
                if self.is_leader() {
                    out.push(InternOut::Broadcast(BroadcastToTabs::Alive(Some(true))));
                }
            }
            BroadcastFromTabs::Stopped(tab_id) => {
                self.alive.retain(|id, _| id != &tab_id);
                out.extend(self.msg_timer().await);
            }
            BroadcastFromTabs::ToLeader { from: _, data } => {
                if self.is_leader() {
                    if let Some(danu) = self.danu.as_mut() {
                        if let Err(e) = match &data {
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
                            MsgToLeader::GetUpdate => Ok(out.extend(self.cache.replay())),
                        } {
                            log::error!("While passing {data:?} to danu: {e:?}");
                        }
                    }
                }
            }
            BroadcastFromTabs::FromLeader(data) => {
                if !self.is_leader() {
                    if let Err(e) = match data {
                        MsgFromLeader::DHTStorage(dhts) => self.dht_storage.emit_msg_out(dhts),
                        MsgFromLeader::DHTRouter(dhtr) => self.dht_router.emit_msg_out(dhtr),
                        MsgFromLeader::SystemConfig(fledger_config) => self
                            .network
                            .emit_msg_out(NetworkOut::SystemConfig(fledger_config)),
                        MsgFromLeader::NodeListFromWS(node_infos) => self
                            .network
                            .emit_msg_out(NetworkOut::NodeListFromWS(node_infos)),
                    } {
                        log::error!("While passing data to brokers: {e:?}");
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
                InternIn::Timer(timer_message) => {
                    if timer_message == TimerMessage::Second {
                        out.extend(self.msg_timer().await);
                    }
                }
                InternIn::Broadcast(broadcast_io) => {
                    out.extend(self.msg_broadcast(broadcast_io).await)
                }
                InternIn::FromNode(msg) => {
                    if let Some(msg) = self.cache.update(msg) {
                        if self.is_leader() {
                            out.push(InternOut::Broadcast(BroadcastToTabs::FromLeader(msg)));
                        }
                    }
                }
            }
        }
        out
    }
}

#[derive(Default)]
struct Cache {
    config: Option<FledgerConfig>,
    realm_ids: Vec<RealmID>,
    nodes_connected_dht: Vec<NodeID>,
    nodes_online: Vec<NodeInfo>,
}

impl Cache {
    fn update(&mut self, msg: MsgFromLeader) -> Option<MsgFromLeader> {
        match msg.clone() {
            MsgFromLeader::SystemConfig(conf) => self.config = Some(conf),
            MsgFromLeader::NodeListFromWS(node_infos) => self.nodes_online = node_infos,
            MsgFromLeader::DHTRouter(DHTRouterOut::NodeList(nodes)) => {
                self.nodes_connected_dht = nodes
            }
            MsgFromLeader::DHTStorage(DHTStorageOut::RealmIDs(ids)) => self.realm_ids = ids,
            _ => return None,
        }
        Some(msg)
    }

    fn replay(&self) -> Vec<InternOut> {
        if let Some(config) = self.config.as_ref() {
            vec![
                MsgFromLeader::DHTStorage(DHTStorageOut::RealmIDs(self.realm_ids.clone())),
                MsgFromLeader::DHTRouter(DHTRouterOut::NodeList(self.nodes_connected_dht.clone())),
                MsgFromLeader::SystemConfig(config.clone()),
                MsgFromLeader::NodeListFromWS(self.nodes_online.clone()),
            ]
            .into_iter()
            .map(|m| InternOut::Broadcast(BroadcastToTabs::FromLeader(m)))
            .collect::<Vec<_>>()
        } else {
            return vec![];
        }
    }
}

/// Unique identifier for a node, used for leader election.
/// Lower NodeID = higher priority for leadership.
#[wasm_bindgen]
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
        write!(
            f,
            "{:04}.{:04}",
            self.timestamp_secs % 10000,
            self.random_component / 10000
        )
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
        let mut int0 = Intern::start(
            cfg.clone(),
            tab0.id.clone(),
            Broker::new(),
            Broker::new(),
            Broker::new(),
        )
        .await?;
        let mut int1 = Intern::start(
            cfg,
            tab1.id.clone(),
            Broker::new(),
            Broker::new(),
            Broker::new(),
        )
        .await?;
        let mut itap0 = int0.get_tap_out().await?.0;
        let mut itap1 = int1.get_tap_out().await?.0;

        for i in 0..3 {
            int0.emit_msg_in(InternIn::Start)?;
            let msg = itap0.recv().await.unwrap();
            assert_eq!(InternOut::Broadcast(BroadcastToTabs::Alive(None)), msg);
            int1.emit_msg_in(InternIn::Broadcast(BroadcastFromTabs::Alive {
                from: tab0.id,
                is_leader: None,
            }))?;
            if i == 0 {
                assert_eq!(
                    InternOut::Proxy(ProxyOut::TabList(vec![tab0.id.clone(), tab1.id.clone()])),
                    itap1.recv().await.unwrap()
                );
            }

            int1.emit_msg_in(InternIn::Start)?;
            let msg = itap1.recv().await.unwrap();
            assert_eq!(InternOut::Broadcast(BroadcastToTabs::Alive(None)), msg);
            int0.emit_msg_in(InternIn::Broadcast(BroadcastFromTabs::Alive {
                from: tab1.id,
                is_leader: None,
            }))?;
            if i == 0 {
                assert_eq!(
                    InternOut::Proxy(ProxyOut::TabList(vec![tab0.id.clone(), tab1.id.clone()])),
                    itap0.recv().await.unwrap()
                );
            }
        }

        int0.emit_msg_in(InternIn::Start)?;
        let msg = itap0.recv().await.unwrap();
        assert_eq!(InternOut::Proxy(ProxyOut::Elected), msg);

        int1.emit_msg_in(InternIn::Start)?;
        let msg = itap1.recv().await.unwrap();
        assert_eq!(InternOut::Broadcast(BroadcastToTabs::Alive(None)), msg);

        Ok(())
    }
}
