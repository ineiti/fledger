use flarch::{
    broker::{Broker, SubsystemHandler},
    platform_async_trait,
};
use flmodules::{
    dht_router::broker::{DHTRouterIn, DHTRouterOut},
    dht_storage::broker::{DHTStorageIn, DHTStorageOut},
    network::signal::FledgerConfig,
    nodeconfig::NodeInfo,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
use web_sys::BroadcastChannel;

use crate::proxy::intern::TabID;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum MsgToLeader {
    DHTStorage(DHTStorageIn),
    DHTRouter(DHTRouterIn),
    GetUpdate,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum MsgFromLeader {
    DHTStorage(DHTStorageOut),
    DHTRouter(DHTRouterOut),
    SystemConfig(FledgerConfig),
    NodeListFromWS(Vec<NodeInfo>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BroadcastToTabs {
    Alive,
    Stopped,
    ToLeader(MsgToLeader),
    FromLeader(MsgFromLeader),
}

impl BroadcastToTabs {
    /// Returns `true` if the broadcast to tabs is [`ToLeader`].
    ///
    /// [`ToLeader`]: BroadcastToTabs::ToLeader
    #[must_use]
    pub fn is_to_leader(&self) -> bool {
        matches!(self, Self::ToLeader(..))
    }

    /// Returns `true` if the broadcast to tabs is [`FromLeader`].
    ///
    /// [`FromLeader`]: BroadcastToTabs::FromLeader
    #[must_use]
    pub fn is_from_leader(&self) -> bool {
        matches!(self, Self::FromLeader(..))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BroadcastFromTabs {
    Alive(TabID),
    Stopped(TabID),
    ToLeader { from: TabID, data: MsgToLeader },
    FromLeader(MsgFromLeader),
}

pub type BrokerBroadcast = Broker<BroadcastToTabs, BroadcastFromTabs>;

#[derive(Debug, Clone)]
pub struct Broadcast {
    id: TabID,
    channel: BroadcastChannel,
}

impl Broadcast {
    fn convert_message(id: TabID, msg: BroadcastToTabs) -> BroadcastFromTabs {
        match msg {
            BroadcastToTabs::Alive => BroadcastFromTabs::Alive(id),
            BroadcastToTabs::Stopped => BroadcastFromTabs::Stopped(id),
            BroadcastToTabs::ToLeader(data) => BroadcastFromTabs::ToLeader { from: id, data },
            BroadcastToTabs::FromLeader(data) => BroadcastFromTabs::FromLeader(data),
        }
    }

    pub async fn start(channel_name: &str, id: TabID) -> anyhow::Result<BrokerBroadcast> {
        let channel = BroadcastChannel::new(channel_name).map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let mut broker = Broker::new();
        let mut br_cl = broker.clone();
        let onmessage = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
            move |event: web_sys::MessageEvent| {
                if let Some(data_str) = event.data().as_string() {
                    if let Ok(msg) = serde_json::from_str::<BroadcastFromTabs>(&data_str) {
                        if let Err(e) = br_cl.emit_msg_out(msg) {
                            log::error!("While sending broadcast message: {e}");
                        }
                    }
                }
            },
        );
        channel.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();
        broker
            .add_handler(Box::new(Broadcast { id, channel }))
            .await?;

        Ok(broker)
    }
}

#[platform_async_trait]
impl SubsystemHandler<BroadcastToTabs, BroadcastFromTabs> for Broadcast {
    async fn messages(&mut self, msgs: Vec<BroadcastToTabs>) -> Vec<BroadcastFromTabs> {
        for msg in msgs {
            let out = Self::convert_message(self.id, msg);
            if let Ok(json) = serde_json::to_string(&out) {
                if let Err(e) = self.channel.post_message(&JsValue::from_str(&json)) {
                    log::error!("Couldn't send to broadcast: {e:?}");
                }
            }
        }
        vec![]
    }
}

#[cfg(test)]
pub mod test {
    use std::collections::HashMap;

    use flarch::broker::Broker;
    use tokio::sync::mpsc::UnboundedReceiver;

    use super::*;

    #[derive(Debug)]
    pub struct Tab {
        pub id: TabID,
        pub tap: UnboundedReceiver<BroadcastFromTabs>,
        pub broker: BrokerBroadcast,
    }

    impl Tab {
        pub async fn new() -> anyhow::Result<Self> {
            let mut broker = Broker::new();
            let tap = broker.get_tap_out().await?.0;
            Ok(Self {
                id: TabID::new(),
                tap,
                broker,
            })
        }

        pub async fn new_const(id: TabID) -> anyhow::Result<Self> {
            let mut broker = Broker::new();
            let tap = broker.get_tap_out().await?.0;
            Ok(Self { id, tap, broker })
        }

        pub fn send(&mut self, msg: BroadcastToTabs) -> anyhow::Result<()> {
            self.broker.emit_msg_in(msg)
        }

        pub async fn recv(&mut self) -> anyhow::Result<BroadcastFromTabs> {
            self.tap.recv().await.ok_or(anyhow::anyhow!("Rx error"))
        }
    }

    #[derive(Debug, Clone, Default)]
    pub struct BroadcastTest {
        brokers: HashMap<TabID, BrokerBroadcast>,
    }

    impl BroadcastTest {
        pub async fn new(&mut self) -> anyhow::Result<Tab> {
            let mut tab = Tab::new().await?;
            for b in &mut self.brokers {
                tab.broker
                    .add_translator_i_to(
                        b.1.clone(),
                        Box::new(move |m| Some(Broadcast::convert_message(tab.id, m))),
                    )
                    .await?;
                let id = b.0.clone();
                b.1.add_translator_i_to(
                    tab.broker.clone(),
                    Box::new(move |m| Some(Broadcast::convert_message(id, m))),
                )
                .await?;
            }
            self.brokers.insert(tab.id, tab.broker.clone());
            Ok(tab)
        }
    }

    use wasm_bindgen_test::wasm_bindgen_test;
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test(async)]
    async fn test_broadcast() -> anyhow::Result<()> {
        let mut channels = BroadcastTest::default();
        let mut tab0 = channels.new().await?;
        let mut tab1 = channels.new().await?;

        tab0.send(BroadcastToTabs::Alive)?;
        assert_eq!(BroadcastFromTabs::Alive(tab0.id), tab1.recv().await?);

        tab1.send(BroadcastToTabs::Alive)?;
        assert_eq!(BroadcastFromTabs::Alive(tab1.id), tab0.recv().await?);
        Ok(())
    }
}
