use flarch::broker::{Broker, TranslateFrom, TranslateInto};
use tokio::sync::watch;

use crate::{
    random_connections::broker::{BrokerRandom, RandomIn, RandomOut},
    router::messages::NetworkWrapper,
    timer::Timer,
};

use super::{
    core::PingStorage,
    messages::{Messages, PingConfig, PingIn, PingOut},
};

const MODULE_NAME: &str = "Ping";

pub type BrokerPing = Broker<PingIn, PingOut>;

/// This links the Ping module with a RandomConnections module, so that
/// all messages are correctly translated from one to the other.
pub struct Ping {
    /// Represents the underlying broker.
    pub broker: BrokerPing,
    /// Is used to pass the PingStorage structure from the Transalate to the PingBroker.
    pub storage: watch::Receiver<PingStorage>,
}

impl Ping {
    pub async fn start(
        config: PingConfig,
        rc: BrokerRandom,
        timer: &mut Timer,
    ) -> anyhow::Result<Self> {
        let (messages, storage) = Messages::new(config);
        let mut broker = Broker::new();
        broker.add_handler(Box::new(messages)).await?;
        broker.link_bi(rc).await?;
        timer.tick_second(broker.clone(), PingIn::Tick).await?;

        Ok(Ping { storage, broker })
    }
}

impl TranslateFrom<RandomOut> for PingIn {
    fn translate(msg: RandomOut) -> Option<Self> {
        match msg {
            RandomOut::DisconnectNode(id) => Some(PingIn::DisconnectNode(id)),
            RandomOut::NodeIDsConnected(list) => Some(PingIn::UpdateNodeList(list.into())),
            RandomOut::NetworkWrapperFromNetwork(id, msg) => msg
                .unwrap_yaml(MODULE_NAME)
                .map(|msg| PingIn::FromNetwork(id, msg)),
            _ => None,
        }
    }
}

impl TranslateInto<RandomIn> for PingOut {
    fn translate(self) -> Option<RandomIn> {
        Some(match self {
            PingOut::ToNetwork(id, msg_node) => RandomIn::NetworkWrapperToNetwork(
                id,
                NetworkWrapper::wrap_yaml(MODULE_NAME, &msg_node).unwrap(),
            ),
            PingOut::Failed(id) => RandomIn::NodeFailure(id).into(),
        })
    }
}
