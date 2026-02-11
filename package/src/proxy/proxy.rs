use flarch::{
    add_translator_direct, add_translator_link,
    broker::Broker,
    tasks::{spawn_local, wait_ms},
};
use flmodules::{
    dht_router::broker::BrokerDHTRouter, dht_storage::broker::BrokerDHTStorage,
    network::broker::BrokerNetwork, timer::BrokerTimer,
};

use crate::{
    danode::NetConf,
    proxy::{
        broadcast::BrokerBroadcast,
        intern::{BrokerIntern, Intern, InternIn, InternOut, TabID},
    },
};

#[derive(Debug, Clone, PartialEq)]
pub enum ProxyIn {}

#[derive(Debug, Clone, PartialEq)]
pub enum ProxyOut {
    Elected,
    NewLeader(TabID),
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
    intern: BrokerIntern,
}

impl Proxy {
    pub async fn start(
        id: TabID,
        cfg: NetConf,
        timer: &mut BrokerTimer,
        broadcast: BrokerBroadcast,
    ) -> anyhow::Result<Proxy> {
        let mut intern = Intern::start(cfg, id).await?;
        add_translator_link!(intern, broadcast, InternIn::Broadcast, InternOut::Broadcast);

        timer
            .add_translator_o_ti(intern.clone(), Box::new(|t| Some(InternIn::Timer(t))))
            .await?;
        let broker = BrokerProxy::new();
        add_translator_direct!(intern, broker.clone(), InternIn::Proxy, InternOut::Proxy);
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
            intern,
        };

        Ok(proxy)
    }

    pub fn elect_leader(&mut self) {
        let mut int = self.intern.clone();
        spawn_local(async move {
            for i in 0..4 {
                if let Err(e) = int.emit_msg_in(InternIn::Start) {
                    log::error!("While sending InternIn::Start: {e:?}");
                }
                if i < 3 {
                    wait_ms(500).await;
                }
            }
        });
    }
}

#[cfg(test)]
mod test {
    use flmodules::timer::BrokerTimer;
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::*;

    use crate::proxy::broadcast::test::BroadcastTest;

    #[wasm_bindgen_test(async)]
    async fn test_broadcast() -> anyhow::Result<()> {
        let mut _channels = BroadcastTest::default();
        let mut _tab0 = _channels.new().await?;
        let mut _tab1 = _channels.new().await?;
        let mut _timer = BrokerTimer::new();
        let cfg = NetConf::default();
        let mut _proxy0 =
            Proxy::start(_tab0.id, cfg.clone(), &mut _timer, _tab0.broker.clone()).await?;
        let mut _proxy1 = Proxy::start(_tab1.id, cfg, &mut _timer, _tab1.broker.clone()).await?;

        Ok(())
    }
}
