use std::sync::mpsc::{channel, Receiver, Sender};

use flarch::{
    broker::{self, Broker, BrokerError, Subsystem, SubsystemHandler},
    nodeids::U256,
    platform_async_trait,
};

use crate::{
    network::messages::{NetworkIn, NetworkOut, NetworkMessage},
    random_connections::{
        messages::{Config, ModuleMessage, RandomConnections, RandomIn, RandomMessage, RandomOut},
        core::RandomStorage,
    },
    timer::TimerMessage,
};

pub struct RandomBroker {
    pub storage: RandomStorage,
    pub broker: Broker<RandomMessage>,
    storage_rx: Receiver<RandomStorage>,
}

impl RandomBroker {
    pub async fn start(id: U256, broker_net: Broker<NetworkMessage>) -> Result<Self, BrokerError> {
        let (storage_tx, storage_rx) = channel();
        let broker = Translate::start(broker_net, storage_tx, id).await?;
        Ok(Self {
            storage: RandomStorage::default(),
            storage_rx,
            broker,
        })
    }

    pub async fn add_timer(&mut self, mut broker: Broker<TimerMessage>) {
        broker
            .forward(
                self.broker.clone(),
                Box::new(|msg: TimerMessage| {
                    matches!(msg, TimerMessage::Second).then(|| RandomIn::Tick.into())
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

pub struct Translate {
    storage_tx: Sender<RandomStorage>,
    module: RandomConnections,
    id: U256,
}

impl Translate {
    pub async fn start(
        broker_net: Broker<NetworkMessage>,
        storage_tx: Sender<RandomStorage>,
        id: U256,
    ) -> Result<Broker<RandomMessage>, BrokerError> {
        let mut rc = Broker::new();
        rc.add_subsystem(Subsystem::Handler(Box::new(Translate {
            storage_tx,
            module: RandomConnections::new(Config::default()),
            id,
        })))
        .await?;
        rc.link_bi(
            broker_net,
            Self::link_net_rnd(id),
            Box::new(Self::link_rnd_net),
        )
        .await?;
        Ok(rc)
    }

    fn link_net_rnd(our_id: U256) -> broker::Translate<NetworkMessage, RandomMessage> {
        Box::new(move |msg: NetworkMessage| {
            if let NetworkMessage::Output(msg_net) = msg {
                match msg_net {
                    NetworkOut::MessageFromNode(id, msg_str) => {
                        if let Ok(msg_rnd) = serde_yaml::from_str::<ModuleMessage>(&msg_str) {
                            return Some(RandomIn::NodeCommFromNetwork(id, msg_rnd).into());
                        }
                    }
                    NetworkOut::NodeListFromWS(list) => {
                        return Some(
                            RandomIn::NodeList(
                                list.into_iter()
                                    .filter(|ni| ni.get_id() != our_id)
                                    .collect(),
                            )
                            .into(),
                        )
                    }
                    NetworkOut::Connected(id) => return Some(RandomIn::NodeConnected(id).into()),
                    NetworkOut::Disconnected(id) => {
                        return Some(RandomIn::NodeDisconnected(id).into())
                    }
                    _ => {}
                }
            }
            None
        })
    }

    fn link_rnd_net(msg: RandomMessage) -> Option<NetworkMessage> {
        if let RandomMessage::Output(msg_out) = msg {
            match msg_out {
                RandomOut::ConnectNode(id) => return Some(NetworkIn::Connect(id).into()),
                RandomOut::DisconnectNode(id) => return Some(NetworkIn::Disconnect(id).into()),
                RandomOut::NodeCommToNetwork(id, msg) => {
                    let msg_str = serde_yaml::to_string(&msg).unwrap();
                    return Some(NetworkIn::MessageToNode(id, msg_str).into());
                }
                _ => {}
            }
        }
        None
    }

    fn handle_output(&mut self, msg: &RandomOut) {
        if let RandomOut::Storage(s) = msg {
            self.storage_tx
                .send(s.clone())
                .err()
                .map(|e| log::error!("While sending update to random storage: {e:?}"));
        }
    }
}

#[platform_async_trait()]
impl SubsystemHandler<RandomMessage> for Translate {
    async fn messages(&mut self, msgs: Vec<RandomMessage>) -> Vec<RandomMessage> {
        let mut out = vec![];
        for msg in msgs {
            log::trace!("{} processing {msg:?}", self.id);
            if let RandomMessage::Input(msg_in) = msg {
                out.extend(self.module.process_message(msg_in));
            }
        }

        for msg in out.iter() {
            log::trace!("{} outputted {msg:?}", self.id);
            self.handle_output(msg);
        }

        out.into_iter().map(|m| m.into()).collect()
    }
}
