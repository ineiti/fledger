use std::sync::mpsc::{channel, Receiver, Sender};

use flarch::{
    broker::{Broker, BrokerError, Subsystem, SubsystemHandler},
    platform_async_trait,
};

use crate::{
    overlay::messages::NetworkWrapper,
    random_connections::messages::{RandomIn, RandomMessage, RandomOut},
    timer::TimerMessage,
};

use super::{
    core::PingStorage,
    messages::{Ping, PingConfig, PingIn, PingMessage, PingOut},
};

const MODULE_NAME: &str = "Ping";

/// This links the Ping module with a RandomConnections module, so that
/// all messages are correctly translated from one to the other.
pub struct PingBroker {
    /// This is always updated with the latest view of the Ping module.
    pub storage: PingStorage,
    /// Represents the underlying broker.
    pub broker: Broker<PingMessage>,
    /// Is used to pass the PingStorage structure from the Transalate to the PingBroker.
    storage_rx: Receiver<PingStorage>,
}

impl PingBroker {
    pub async fn start(config: PingConfig, rc: Broker<RandomMessage>) -> Result<Self, BrokerError> {
        let (storage_tx, storage_rx) = channel();
        let broker = Translate::start(rc, config.clone(), storage_tx).await?;
        Ok(PingBroker {
            storage: PingStorage::new(config),
            storage_rx,
            broker,
        })
    }

    pub async fn add_timer(&mut self, mut timer: Broker<TimerMessage>) {
        timer
            .forward(
                self.broker.clone(),
                Box::new(|msg: TimerMessage| {
                    matches!(msg, TimerMessage::Second).then(|| PingIn::Tick.into())
                }),
            )
            .await;
    }

    pub fn update(&mut self) {
        for update in self.storage_rx.try_iter() {
            self.storage = update;
        }
    }
}

struct Translate {
    storage_tx: Sender<PingStorage>,
    module: Ping,
}

impl Translate {
    async fn start(
        random: Broker<RandomMessage>,
        config: PingConfig,
        storage_tx: Sender<PingStorage>,
    ) -> Result<Broker<PingMessage>, BrokerError> {
        let mut gossip = Broker::new();
        gossip
            .add_subsystem(Subsystem::Handler(Box::new(Translate {
                storage_tx,
                module: Ping::new(config),
            })))
            .await?;
        gossip
            .link_bi(
                random,
                Box::new(Self::link_rnd_ping),
                Box::new(Self::link_ping_rnd),
            )
            .await?;
        Ok(gossip)
    }

    fn link_rnd_ping(msg: RandomMessage) -> Option<PingMessage> {
        if let RandomMessage::Output(msg_out) = msg {
            match msg_out {
                RandomOut::DisconnectNode(id) => Some(PingIn::DisconnectNode(id).into()),
                RandomOut::NodeIDsConnected(list) => Some(PingIn::NodeList(list.into()).into()),
                RandomOut::NetworkWrapperFromNetwork(id, msg) => msg
                    .unwrap_yaml(MODULE_NAME)
                    .map(|msg| PingIn::FromNetwork(id, msg).into()),
                _ => None,
            }
        } else {
            None
        }
    }

    fn link_ping_rnd(msg: PingMessage) -> Option<RandomMessage> {
        if let PingMessage::Output(msg_out) = msg {
            match msg_out {
                PingOut::ToNetwork(id, msg_node) => Some(
                    RandomIn::NetworkWrapperToNetwork(
                        id,
                        NetworkWrapper::wrap_yaml(MODULE_NAME, &msg_node).unwrap(),
                    )
                    .into(),
                ),
                PingOut::Failed(id) => Some(RandomIn::NodeFailure(id).into()),
                _ => None,
            }
        } else {
            None
        }
    }

    fn handle_output(&mut self, msg_out: &PingOut) {
        if let PingOut::Storage(s) = msg_out {
            self.storage_tx
                .send(s.clone())
                .err()
                .map(|e| log::error!("Couldn't send to storage_tx: {e:?}"));
        }
    }
}

#[platform_async_trait()]
impl SubsystemHandler<PingMessage> for Translate {
    async fn messages(&mut self, msgs: Vec<PingMessage>) -> Vec<PingMessage> {
        let mut out = vec![];
        for msg in msgs {
            log::trace!("Got msg: {msg:?}");
            if let PingMessage::Input(msg_in) = msg {
                out.extend(self.module.process_msg(msg_in));
            }
        }
        for msg in out.iter() {
            log::trace!("Outputting msg: {msg:?}");
            self.handle_output(msg);
        }

        out.into_iter().map(|o| o.into()).collect()
    }
}
