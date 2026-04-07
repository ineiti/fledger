//! Does all the election of the tab-leader, starting of a node for
//! the leader, and updating the state.

use std::collections::HashMap;

use flarch::{
    add_translator,
    broker::{Broker, SubsystemHandler},
    data_storage::{DataStorage, DataStorageIndexedDB},
    platform_async_trait,
    web_rtc::connection::{ConnectionConfig, HostLogin},
};
use flmodules::{
    dht_router::broker::DHTRouterOut, dht_storage::broker::DHTStorageOut,
    network::broker::NetworkOut, nodeconfig::NodeConfig, Modules,
};
use flnode::node;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use web_sys::window;

use crate::{
    danode::NetConf,
    proxy::{
        broadcast::{BroadcastFromTabs, BroadcastToTabs, TabID},
        proxy::NodeIn,
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum NodeOut {
    DHTRouter(DHTRouterOut),
    DHTStorage(DHTStorageOut),
    Network(NetworkOut),
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
    ds: Box<DataStorageIndexedDB>,
    danu: Option<node::Node>,
}

impl Intern {
    pub async fn start(
        mut ds: Box<DataStorageIndexedDB>,
        node_config: NodeConfig,
        id: TabID,
        search_cnt: usize,
        netconf: NetConf,
    ) -> anyhow::Result<(BrokerIntern, watch::Receiver<State>)> {
        let mut broker = Broker::new();
        let state = State::from_storage(&mut ds, None).await?;
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

    async fn tab_alive(&mut self, id: TabID, role: Option<bool>) -> Vec<InternOut> {
        self.tabs.insert(id, TAB_TIMEOUT);
        if let Some(r) = role {
            if r {
                if self.role == TabRole::Leader {
                    log::error!("Found two leaders!");
                    if let Some(window) = window() {
                        let _ = window.location().reload();
                    }
                } else if self.role == TabRole::Search {
                    return self.set_follower(id).await;
                }
            }
        }
        vec![]
    }

    async fn update_state(&mut self) {
        match self.role {
            TabRole::Follower => match State::from_storage(&mut self.ds, Some(false)).await {
                Ok(state) => self.state = state,
                Err(e) => log::error!("While reading state: {e:?}"),
            },
            TabRole::Leader => {
                self.state.store(&mut self.ds).await;
            }
            _ => return,
        }
        if let Err(e) = self.state_send.send(self.state.clone()) {
            log::error!("While sending new state: {e:?}");
        }
    }

    async fn set_follower(&mut self, leader: TabID) -> Vec<InternOut> {
        log::info!("Tab becomes follower");
        self.role = TabRole::Follower;
        self.search_cnt = 0;
        self.update_state().await;
        vec![InternOut::Update(StateUpdate::NewLeader(leader))]
    }

    async fn set_leader(&mut self) -> Vec<InternOut> {
        log::info!("Tab becomes leader");
        self.role = TabRole::Leader;
        self.search_cnt = 0;
        self.state.is_leader = Some(true);
        self.state.nodes_connected_dht.clear();
        self.state.nodes_online.clear();
        self.update_state().await;
        if let Err(e) = self.start_node().await {
            log::error!("Couldn't setup node: {e:?}")
        }
        vec![
            InternOut::Update(StateUpdate::NewLeader(self.id.clone())),
            InternOut::Update(StateUpdate::ConnectSignal(false)),
            InternOut::Update(StateUpdate::ConnectedNodes(0)),
            InternOut::Update(StateUpdate::AvailableNodes(vec![])),
        ]
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
            return self.set_leader().await;
        }
        vec![]
    }

    async fn start_node(&mut self) -> anyhow::Result<()> {
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
            node::Node::start_network(self.ds.clone_box(), self.node_config.clone(), config)
                .await?;

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
                        return self.set_leader().await;
                    } else {
                        out.extend(
                            self.set_follower(self.tabs_sorted().first().unwrap().clone())
                                .await,
                        );
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
        self.update_state().await;
        out.push(InternOut::Broadcast(BroadcastToTabs::Alive(role)));
        out
    }

    async fn msg_follower(&mut self, msg: InternIn) -> Vec<InternOut> {
        match msg {
            InternIn::Broadcast(bc) => match bc {
                BroadcastFromTabs::Alive(id, role) => self.tab_alive(id, role).await,
                BroadcastFromTabs::Stopped(id) => self.tab_stopped(id).await,
                BroadcastFromTabs::FromLeader(id, msg_leader) => {
                    let mut out = vec![InternOut::Update(msg_leader.clone())];
                    if self.role == TabRole::Search {
                        out.extend(self.set_follower(id).await);
                    } else {
                        self.update_state().await;
                    }
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
                BroadcastFromTabs::Alive(id, role) => self.tab_alive(id, role).await,
                BroadcastFromTabs::Stopped(id) => self.tab_stopped(id).await,
                _ => vec![],
            },
            InternIn::Timer => self.tick().await,
            InternIn::NodeOut(node_out) => {
                let out = self.state.msg_node(node_out);
                if !out.is_empty() {
                    self.update_state().await;
                    out.into_iter()
                        .flat_map(|o| vec![InternOut::Update(o.clone())])
                        .collect::<Vec<_>>()
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
            self.update_state().await;
            out.push(InternOut::Update(m));
        }
        if self.role == TabRole::Leader {
            let mut broadcast = vec![];
            for o in &out {
                if let InternOut::Update(u) = o {
                    broadcast.push(InternOut::Broadcast(BroadcastToTabs::FromLeader(u.clone())));
                }
            }
            out.extend(broadcast);
        }
        out
    }
}
