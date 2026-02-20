use std::collections::HashMap;

use flarch::{
    add_translator,
    broker::{Broker, SubsystemHandler},
    data_storage::DataStorage,
    platform_async_trait,
    web_rtc::connection::{ConnectionConfig, HostLogin},
};
use flmodules::{nodeconfig::NodeConfig, Modules};
use flnode::node;
use serde::{Deserialize, Serialize};

use crate::{
    danode::NetConf,
    proxy::{
        broadcast::{BroadcastFromTabs, BroadcastToTabs, TabID},
        proxy::{NodeIn, NodeOut, Tabs},
        state::StateUpdate,
    },
};

const TAB_TIMEOUT: usize = 5;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum InternIn {
    Broadcast(BroadcastFromTabs),
    Node(NodeIn),
    Timer,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum InternOut {
    Broadcast(BroadcastToTabs),
    // Only the leader will ever send Node messages
    Node(NodeOut),
    Tabs(Tabs),
    Update(StateUpdate),
}

pub type BrokerIntern = Broker<InternIn, InternOut>;

#[derive(Debug, PartialEq)]
enum InternState {
    Search,
    Follower,
    Leader,
}

pub struct Intern {
    broker: BrokerIntern,
    state: InternState,
    node_config: NodeConfig,
    search_cnt: usize,
    tabs: HashMap<TabID, usize>,
    id: TabID,
    netconf: NetConf,
    ds: Box<dyn DataStorage + Send>,
    danu: Option<node::Node>,
}

impl Intern {
    pub async fn start(
        ds: Box<dyn DataStorage + Send>,
        node_config: NodeConfig,
        id: TabID,
        search_cnt: usize,
        netconf: NetConf,
    ) -> anyhow::Result<BrokerIntern> {
        let mut broker = Broker::new();
        let intern = Intern {
            state: InternState::Search,
            node_config,
            search_cnt,
            tabs: HashMap::new(),
            id,
            netconf,
            ds,
            broker: broker.clone(),
            danu: None,
        };
        broker.add_handler(Box::new(intern)).await?;
        Ok(broker)
    }

    fn tab_alive(&mut self, id: TabID) -> Vec<InternOut> {
        self.tabs.insert(id, TAB_TIMEOUT);
        vec![]
    }

    fn is_leader(&self) -> bool {
        if let Some(leader) = self.tabs_sorted().first() {
            leader == &self.id
        } else {
            false
        }
    }

    async fn tab_stopped(&mut self, id: TabID) -> Vec<InternOut> {
        self.tabs.remove(&id);
        self.check_leader().await
    }

    async fn check_leader(&mut self) -> Vec<InternOut> {
        if self.state == InternState::Leader || !self.is_leader() {
            vec![]
        } else {
            match self.start_node().await {
                Ok(out) => out,
                Err(err) => {
                    log::error!("Couldn't setup node: {err:?}");
                    vec![]
                }
            }
        }
    }

    async fn start_node(&mut self) -> anyhow::Result<Vec<InternOut>> {
        self.state = InternState::Leader;

        let config = ConnectionConfig::new(
            self.netconf.signal_server.clone(),
            self.netconf
                .stun_server
                .as_ref()
                .and_then(|url| Some(HostLogin::from_url(&url.clone()))),
            self.netconf
                .turn_server
                .as_ref()
                .and_then(|url| HostLogin::from_login_url(&url).ok()),
        );
        self.node_config.info.modules = Modules::stable() - Modules::WEBPROXY_REQUESTS;
        let mut danu =
            node::Node::start_network(self.ds.clone(), self.node_config.clone(), config).await?;

        add_translator!(danu.broker_net, o_to, self.broker,
            msg => InternOut::Node(NodeOut::Network(msg)));

        let dhtr = &mut danu.dht_router.as_mut().unwrap().broker;
        add_translator!(dhtr, o_to, self.broker,
            msg => InternOut::Node(NodeOut::DHTRouter(msg)));
        add_translator!(self.broker, i_ti, dhtr, InternIn::Node(NodeIn::DHTRouter(msg)) => msg);
        add_translator!(self.broker, i_ti, dhtr,
            InternIn::Broadcast(BroadcastFromTabs::ToLeader(NodeIn::DHTRouter(msg))) => msg);

        let dhts = &mut danu.dht_storage.as_mut().unwrap().broker;
        add_translator!(dhts, o_to, self.broker,
            msg => InternOut::Node(NodeOut::DHTStorage(msg)));
        add_translator!(self.broker, i_ti, dhts, InternIn::Node(NodeIn::DHTStorage(msg)) => msg);
        add_translator!(self.broker, i_ti, dhts,
            InternIn::Broadcast(BroadcastFromTabs::ToLeader(NodeIn::DHTStorage(msg))) => msg);

        self.danu = Some(danu);
        Ok(vec![
            InternOut::Tabs(Tabs::Elected),
            InternOut::Tabs(Tabs::NewLeader(self.id)),
        ])
    }

    fn tabs_sorted(&self) -> Vec<TabID> {
        let mut tabs = self.tabs.keys().cloned().collect::<Vec<_>>();
        tabs.sort();
        tabs
    }

    async fn tick(&mut self) -> Vec<InternOut> {
        self.tabs.insert(self.id.clone(), TAB_TIMEOUT);
        match self.state {
            InternState::Search => {
                if self.search_cnt > 0 {
                    self.search_cnt -= 1;
                } else {
                    if self.is_leader() {
                        match self.start_node().await {
                            Ok(out) => return out,
                            Err(err) => {
                                log::error!("Couldn't start node: {err:?}");
                            }
                        }
                    }
                    self.state = InternState::Follower;
                }
            }
            InternState::Follower | InternState::Leader => {
                for tab in &mut self.tabs {
                    *tab.1 -= 1;
                }
                self.tabs.retain(|_, v| *v > 0);
                self.check_leader().await;
            }
        }
        vec![InternOut::Broadcast(BroadcastToTabs::Alive)]
    }

    async fn msg_follower(&mut self, msg: InternIn) -> Vec<InternOut> {
        match msg {
            InternIn::Broadcast(bc) => match bc {
                BroadcastFromTabs::Alive(id) => self.tab_alive(id),
                BroadcastFromTabs::Stopped(id) => self.tab_stopped(id).await,
                BroadcastFromTabs::FromLeader(msg_leader) => {
                    self.search_cnt = 0;
                    self.state = InternState::Follower;
                    vec![InternOut::Update(msg_leader)]
                }
                _ => vec![],
            },
            InternIn::Node(node_in) => {
                vec![InternOut::Broadcast(BroadcastToTabs::ToLeader(node_in))]
            }
            InternIn::Timer => self.tick().await,
        }
    }

    async fn msg_leader(&mut self, msg: InternIn) -> Vec<InternOut> {
        match msg {
            InternIn::Broadcast(bc) => match bc {
                BroadcastFromTabs::Alive(id) => self.tab_alive(id),
                BroadcastFromTabs::Stopped(id) => self.tab_stopped(id).await,
                _ => vec![],
            },
            InternIn::Timer => self.tick().await,
            _ => vec![],
        }
    }
}

#[platform_async_trait]
impl SubsystemHandler<InternIn, InternOut> for Intern {
    async fn messages(&mut self, msgs: Vec<InternIn>) -> Vec<InternOut> {
        let mut out = vec![];
        let tabs = self.tabs_sorted();
        for msg in msgs {
            match self.state {
                InternState::Search | InternState::Follower => {
                    out.extend(self.msg_follower(msg).await)
                }
                InternState::Leader => out.extend(self.msg_leader(msg).await),
            }
        }
        if tabs != self.tabs_sorted() && self.is_leader() {
            out.push(InternOut::Tabs(Tabs::TabList(self.tabs_sorted())));
        }
        out
    }
}
