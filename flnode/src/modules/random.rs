use std::sync::mpsc::{channel, Receiver, Sender};

use async_trait::async_trait;
use flmodules::{
    random_connections::{
        module::{Config, NodeMessage, RandomConnections, RandomIn, RandomMessage, RandomOut},
        storage::RandomStorage,
    },
    timer::TimerMessage,
    broker::{self, Broker, BrokerError, SubsystemHandler},
    nodeids::U256,
};
use flnet::network::{NetCall, NetworkMessage};

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
        rc.add_subsystem(flmodules::broker::Subsystem::Handler(Box::new(Translate {
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
            if let NetworkMessage::Reply(msg_net) = msg {
                match msg_net {
                    flnet::network::NetReply::RcvNodeMessage(id, msg_str) => {
                        if let Ok(msg_rnd) = serde_yaml::from_str::<NodeMessage>(&msg_str) {
                            return Some(RandomIn::NodeMessageFromNetwork((id, msg_rnd)).into());
                        }
                    }
                    flnet::network::NetReply::RcvWSUpdateList(list) => {
                        return Some(
                            RandomIn::NodeList(
                                list.iter()
                                    .map(|n| n.get_id())
                                    .filter(|id| id != &our_id)
                                    .collect::<Vec<U256>>()
                                    .into(),
                            )
                            .into(),
                        )
                    }
                    flnet::network::NetReply::Connected(id) => {
                        return Some(RandomIn::NodeConnected(id).into())
                    }
                    flnet::network::NetReply::Disconnected(id) => {
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
                RandomOut::ConnectNode(id) => return Some(NetCall::Connect(id).into()),
                RandomOut::DisconnectNode(id) => return Some(NetCall::Disconnect(id).into()),
                RandomOut::NodeMessageToNetwork((id, msg)) => {
                    let msg_str = serde_yaml::to_string(&msg).unwrap();
                    return Some(NetCall::SendNodeMessage(id, msg_str).into());
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

#[cfg_attr(feature = "nosend", async_trait(?Send))]
#[cfg_attr(not(feature = "nosend"), async_trait)]
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

        out.into_iter()
            .map(|m| m.into())
            .collect()
    }
}
