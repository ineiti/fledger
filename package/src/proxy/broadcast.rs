use flarch::{
    broker::{Broker, SubsystemHandler},
    platform_async_trait,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
use web_sys::BroadcastChannel;

use crate::proxy::intern::TabID;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum BroadcastToTabs {
    Alive,
    Stopped,
    ToLeader(String),
    FromLeader(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum BroadcastFromTabs {
    Alive(TabID),
    Stopped(TabID),
    ToLeader { from: TabID, data: String },
    FromLeader(String),
}

pub(super) type BrokerBroadcast = Broker<BroadcastToTabs, BroadcastFromTabs>;

#[derive(Debug, Clone)]
pub(super) struct Broadcast {
    id: TabID,
    channel: BroadcastChannel,
}

impl Broadcast {
    pub(super) async fn start(channel_name: &str, id: TabID) -> anyhow::Result<BrokerBroadcast> {
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
            let out = match msg {
                BroadcastToTabs::Alive => BroadcastFromTabs::Alive(self.id.clone()),
                BroadcastToTabs::Stopped => BroadcastFromTabs::Stopped(self.id.clone()),
                BroadcastToTabs::ToLeader(m) => BroadcastFromTabs::ToLeader {
                    from: self.id.clone(),
                    data: m,
                },
                BroadcastToTabs::FromLeader(m) => BroadcastFromTabs::FromLeader(m),
            };
            if let Ok(json) = serde_json::to_string(&out) {
                if let Err(e) = self.channel.post_message(&JsValue::from_str(&json)) {
                    log::error!("Couldn't send to broadcast: {e:?}");
                }
            }
        }
        vec![]
    }
}
