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
use tokio::sync::watch;
use web_sys::window;

use crate::{
    danode::NetConf,
    proxy::{
        broadcast::{BroadcastFromTabs, BroadcastToTabs, TabID},
        proxy::{NodeIn, NodeOut},
        state::{State, StateUpdate},
    },
};

const TAB_TIMEOUT: usize = 5;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum InternIn {
    Broadcast(BroadcastFromTabs),
    NodeIn(NodeIn),
    NodeOut(NodeOut),
    Timer,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum InternOut {
    Broadcast(BroadcastToTabs),
    Update(StateUpdate),
}

pub type BrokerIntern = Broker<InternIn, InternOut>;

#[derive(Debug, PartialEq)]
enum TabRole {
    Search,
    Follower,
    Leader,
}

#[derive(Debug)]
pub struct Intern {
    broker: BrokerIntern,
    role: TabRole,
    state: State,
    state_send: watch::Sender<State>,
    node_config: NodeConfig,
    counter: usize,
    search_cnt: usize,
    tabs: HashMap<TabID, usize>,
    id: TabID,
    netconf: NetConf,
    ds: Box<dyn DataStorage + Send>,
    danu: Option<node::Node>,
}

impl Intern {
    pub async fn start(
        mut ds: Box<dyn DataStorage + Send>,
        node_config: NodeConfig,
        id: TabID,
        search_cnt: usize,
        netconf: NetConf,
    ) -> anyhow::Result<(BrokerIntern, watch::Receiver<State>)> {
        let mut broker = Broker::new();
        let state = State::read(&mut ds, None)?;
        let (state_send, state_rcv) = watch::channel(state.clone());
        let intern = Intern {
            role: TabRole::Search,
            node_config,
            counter: 0,
            search_cnt,
            state,
            state_send,
            tabs: HashMap::new(),
            id,
            netconf,
            ds,
            broker: broker.clone(),
            danu: None,
        };
        broker.add_handler(Box::new(intern)).await?;
        Ok((broker, state_rcv))
    }

    fn tab_alive(&mut self, id: TabID, role: Option<bool>) -> Vec<InternOut> {
        self.tabs.insert(id, TAB_TIMEOUT);
        if let Some(r) = role {
            if r {
                if self.role == TabRole::Leader {
                    log::error!("Found two leaders!");
                    if let Some(window) = window() {
                        let _ = window.location().reload();
                    }
                } else if self.role == TabRole::Search {
                    return self.set_follower();
                }
            }
        }
        vec![]
    }

    fn update_state(&mut self) {
        match self.role {
            TabRole::Follower => match State::read(&mut self.ds, Some(false)) {
                Ok(state) => self.state = state,
                Err(e) => log::error!("While reading state: {e:?}"),
            },
            TabRole::Leader => {
                self.state.store(&mut self.ds);
            }
            _ => return,
        }
        if let Err(e) = self.state_send.send(self.state.clone()) {
            log::error!("While sending new state: {e:?}");
        }
    }

    fn set_follower(&mut self) -> Vec<InternOut> {
        self.role = TabRole::Follower;
        self.search_cnt = 0;
        self.update_state();
        vec![InternOut::Update(StateUpdate::NewLeader)]
    }

    fn set_leader(&mut self) -> Vec<InternOut> {
        self.role = TabRole::Leader;
        self.search_cnt = 0;
        self.state.is_leader = Some(true);
        self.update_state();
        vec![InternOut::Update(StateUpdate::NewLeader)]
    }

    fn is_first_tab(&self) -> bool {
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
        if self.role != TabRole::Leader && self.is_first_tab() {
            if let Err(e) = self.start_node().await {
                log::error!("Couldn't setup node: {e:?}")
            }
            return self.set_leader();
        }
        vec![]
    }

    async fn start_node(&mut self) -> anyhow::Result<()> {
        log::info!("Starting tab as leader");

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

        add_translator!(danu.broker_net, o_ti, self.broker,
            msg => InternIn::NodeOut(NodeOut::Network(msg)));

        let dhtr = &mut danu.dht_router.as_mut().unwrap().broker;
        add_translator!(dhtr, o_ti, self.broker,
            msg => InternIn::NodeOut(NodeOut::DHTRouter(msg)));
        add_translator!(self.broker, i_ti, dhtr, InternIn::NodeIn(NodeIn::DHTRouter(msg)) => msg);
        add_translator!(self.broker, i_ti, dhtr,
            InternIn::Broadcast(BroadcastFromTabs::ToLeader(NodeIn::DHTRouter(msg))) => msg);

        let dhts = &mut danu.dht_storage.as_mut().unwrap().broker;
        add_translator!(dhts, o_ti, self.broker,
            msg => InternIn::NodeOut(NodeOut::DHTStorage(msg)));
        add_translator!(self.broker, i_ti, dhts, InternIn::NodeIn(NodeIn::DHTStorage(msg)) => msg);
        add_translator!(self.broker, i_ti, dhts,
            InternIn::Broadcast(BroadcastFromTabs::ToLeader(NodeIn::DHTStorage(msg))) => msg);

        self.danu = Some(danu);
        Ok(())
    }

    fn tabs_sorted(&self) -> Vec<TabID> {
        let mut tabs = self.tabs.keys().cloned().collect::<Vec<_>>();
        tabs.sort();
        tabs
    }

    async fn tick(&mut self) -> Vec<InternOut> {
        self.counter += 1;
        self.tabs.insert(self.id.clone(), TAB_TIMEOUT);
        let mut out = vec![];
        match self.role {
            TabRole::Search => {
                if self.counter >= self.search_cnt {
                    if self.is_first_tab() {
                        if let Err(e) = self.start_node().await {
                            log::error!("Couldn't start node: {e:?}");
                        }
                        return self.set_leader();
                    } else {
                        out.extend(self.set_follower());
                    }
                }
            }
            TabRole::Follower | TabRole::Leader => {
                for tab in &mut self.tabs {
                    *tab.1 -= 1;
                }
                self.tabs.retain(|_, v| *v > 0);
                if self.role == TabRole::Follower {
                    out.extend(self.check_leader().await);
                }
            }
        }
        let role = match self.role {
            TabRole::Follower => Some(false),
            TabRole::Leader => Some(true),
            _ => None,
        };
        out.push(InternOut::Broadcast(BroadcastToTabs::Alive(role)));
        out
    }

    async fn msg_follower(&mut self, msg: InternIn) -> Vec<InternOut> {
        match msg {
            InternIn::Broadcast(bc) => match bc {
                BroadcastFromTabs::Alive(id, role) => self.tab_alive(id, role),
                BroadcastFromTabs::Stopped(id) => self.tab_stopped(id).await,
                BroadcastFromTabs::FromLeader(msg_leader) => {
                    log::info!("Got msg from leader: {msg_leader:?}");
                    let mut out = self.set_follower();
                    out.push(InternOut::Update(msg_leader));
                    out
                }
                _ => vec![],
            },
            InternIn::NodeIn(node_in) => {
                vec![InternOut::Broadcast(BroadcastToTabs::ToLeader(node_in))]
            }
            InternIn::Timer => self.tick().await,
            _ => vec![],
        }
    }

    async fn msg_leader(&mut self, msg: InternIn) -> Vec<InternOut> {
        match msg {
            InternIn::Broadcast(bc) => match bc {
                BroadcastFromTabs::Alive(id, role) => self.tab_alive(id, role),
                BroadcastFromTabs::Stopped(id) => self.tab_stopped(id).await,
                _ => vec![],
            },
            InternIn::Timer => self.tick().await,
            InternIn::NodeOut(node_out) => {
                if let Some(m) = self.state.msg_node(node_out) {
                    self.update_state();
                    vec![InternOut::Update(m)]
                } else {
                    vec![]
                }
            }
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
            match self.role {
                TabRole::Search | TabRole::Follower => out.extend(self.msg_follower(msg).await),
                TabRole::Leader => out.extend(self.msg_leader(msg).await),
            }
        }
        if tabs != self.tabs_sorted() {
            let m = self.state.msg_new_tabs(self.tabs_sorted());
            self.update_state();
            out.push(InternOut::Update(m));
        }
        out
    }
}
