use flarch::{add_translator_direct, add_translator_link, broker::Broker};
use flmodules::{
    dht_router::broker::BrokerDHTRouter, dht_storage::broker::BrokerDHTStorage,
    network::broker::BrokerNetwork, timer::BrokerTimer,
};

use crate::proxy::{
    broadcast::Broadcast,
    intern::{Intern, InternIn, InternOut, TabID},
};

#[derive(Debug, Clone)]
pub enum ProxyIn {}

#[derive(Debug, Clone)]
pub enum ProxyOut {
    Elected,
    TabList(Vec<TabID>),
}

pub type BrokerProxy = Broker<ProxyIn, ProxyOut>;

pub struct Proxy {
    pub broker: BrokerProxy,
    // dht_storage is linked over the broadcastChannel to the
    // actual dht_storage broker on the active danu.
    pub dht_storage: BrokerDHTStorage,
    // dht_router only gets DHTRouterOut messages from the
    // leader. DHTRouterIn messages are ignored.
    pub dht_router: BrokerDHTRouter,
    // network only sends NetworkOut::SystemConfig and NetworkOut::NodeListFromWS.
    // All other messages, including NetworkIn, are ignored.
    pub network: BrokerNetwork,
}

impl Proxy {
    pub async fn start(timer: &mut BrokerTimer) -> anyhow::Result<Proxy> {
        let id = TabID::new();
        let mut intern = Intern::start(id).await?;
        timer
            .add_translator_o_ti(intern.clone(), Box::new(|t| Some(InternIn::Timer(t))))
            .await?;
        let broker = BrokerProxy::new();
        add_translator_direct!(intern, broker.clone(), InternIn::Proxy, InternOut::Proxy);
        let broadcast = Broadcast::start("danu_proxy", id).await?;
        add_translator_link!(intern, broadcast, InternIn::Broadcast, InternOut::Broadcast);
        let dht_storage = BrokerDHTStorage::new();
        add_translator_direct!(
            intern,
            dht_storage.clone(),
            InternIn::DHTStorage,
            InternOut::DHTStorage
        );
        let dht_router = BrokerDHTRouter::new();
        intern
            .add_translator_o_to(
                dht_router.clone(),
                Box::new(|o| match o {
                    InternOut::DHTRouter(dht_router_out) => Some(dht_router_out),
                    _ => None,
                }),
            )
            .await?;
        let network = BrokerNetwork::new();
        intern
            .add_translator_o_to(
                network.clone(),
                Box::new(|o| match o {
                    InternOut::Network(net) => Some(net),
                    _ => None,
                }),
            )
            .await?;

        let proxy = Proxy {
            broker,
            dht_storage,
            dht_router,
            network,
        };

        Ok(proxy)
    }
}
