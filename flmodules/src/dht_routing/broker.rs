use flarch::{
    broker::{self, BrokerError},
    broker_io::{BrokerID, BrokerIO, SubsystemTranslator},
    data_storage::DataStorage,
    nodeids::U256,
    platform_async_trait,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use tokio::sync::watch;

use crate::overlay::messages::{NetworkWrapper, OverlayIn, OverlayOut};
use flarch::{broker::Broker, nodeids::NodeID};

use super::{
    core::{DHTRoutingConfig, DHTRoutingStorage, DHTRoutingStorageSave},
    messages::{DHTRoutingIn, DHTRoutingMessages, DHTRoutingOut, ModuleMessage},
};

const MODULE_NAME: &str = "DHTRouting";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DHTRoutingMessage {
    Store(NodeID, U256, NetworkWrapper),
    UpdateStorage(DHTRoutingStorage),
}

/// This links the DHTRouting module with other modules, so that
/// all messages are correctly translated from one to the other.
/// For this example, it uses the RandomConnections module to communicate
/// with other nodes.
///
/// The [DHTRouting] holds the [Translate] and offers convenience methods
/// to interact with [Translate] and [DHTRoutingMessage].
pub struct DHTRouting {
    pub overlay: BrokerIO<OverlayIn, OverlayOut>,
    pub output: Broker<DHTRoutingMessage>,
    _storage: watch::Receiver<DHTRoutingStorage>,
}

impl DHTRouting {
    pub async fn start(
        ds: Box<dyn DataStorage + Send>,
        our_id: NodeID,
        net: BrokerIO<OverlayIn, OverlayOut>,
        config: DHTRoutingConfig,
    ) -> Result<Self, Box<dyn Error>> {
        let str = ds.get(MODULE_NAME).unwrap_or("".into());
        let storage = DHTRoutingStorageSave::from_str(&str).unwrap_or_default();
        let messages = DHTRoutingMessages::new(storage.clone(), config, our_id)?;
        let (tx, storage) = watch::channel(storage);

        let mut dht_routing = BrokerIO::<DHTRoutingIn, DHTRoutingOut>::new();
        let overlay = BrokerIO::<OverlayIn, OverlayOut>::new();

        dht_routing.add_handler(Box::new(messages)).await?;
        dht_routing.link_bi(net.clone()).await?;
        dht_routing.link_direct(overlay.clone()).await?;
        let output = Broker::new();
        dht_routing.add_translator(Box::new(DHTForwarder {
            output: output.clone(),
            tx,
            ds,
        })).await?;

        Ok(DHTRouting {
            overlay,
            output,
            _storage: storage,
        })
    }
}

impl TryFrom<OverlayOut> for DHTRoutingIn {
    type Error = BrokerError;

    fn try_from(msg_out: OverlayOut) -> Result<Self, Self::Error> {
        match msg_out {
            OverlayOut::NodeIDsConnected(list) => Some(DHTRoutingIn::UpdateNodeList(list.into())),
            OverlayOut::NetworkWrapperFromNetwork(id, msg) => msg
                .unwrap_yaml(MODULE_NAME)
                .map(|msg| DHTRoutingIn::FromNetwork(id, msg)),
            _ => None,
        }
        .ok_or(BrokerError::Translation)
    }
}

impl TryInto<OverlayIn> for DHTRoutingOut {
    type Error = BrokerError;

    fn try_into(self) -> Result<OverlayIn, Self::Error> {
        if let DHTRoutingOut::ToNetwork(id, msg_node) = self {
            Ok(OverlayIn::NetworkWrapperToNetwork(
                id,
                NetworkWrapper::wrap_yaml(MODULE_NAME, &msg_node).unwrap(),
            ))
        } else {
            Err(BrokerError::Translation)
        }
    }
}

impl TryInto<OverlayOut> for DHTRoutingOut {
    type Error = BrokerError;

    fn try_into(self) -> Result<OverlayOut, Self::Error> {
        Err(BrokerError::Translation)
    }
}

impl TryInto<OverlayIn> for DHTRoutingIn {
    type Error = BrokerError;

    fn try_into(self) -> Result<OverlayIn, Self::Error> {
        Err(BrokerError::Translation)
    }
}
/*
    fn link_overlay_dhtrouting(msg: OverlayMessage) -> Option<DHTRoutingMessageIntern> {
        if let OverlayMessage::Output(msg_out) = msg {
            match msg_out {
                OverlayOut::NodeIDsConnected(list) => {
                    Some(DHTRoutingIn::UpdateNodeList(list.into()).into())
                }
                OverlayOut::NetworkWrapperFromNetwork(id, msg) => msg
                    .unwrap_yaml(MODULE_NAME)
                    .map(|msg| DHTRoutingIn::FromNetwork(id, msg).into()),
                _ => None,
            }
        } else {
            None
        }
    }

    fn link_dhtrouting_overlay(msg: DHTRoutingMessageIntern) -> Option<OverlayMessage> {
        if let DHTRoutingMessageIntern::Output(DHTRoutingOut::ToNetwork(id, msg_node)) = msg {
            Some(
                OverlayIn::NetworkWrapperToNetwork(
                    id,
                    NetworkWrapper::wrap_yaml(MODULE_NAME, &msg_node).unwrap(),
                )
                .into(),
            )
        } else {
            None
        }
    }

*/

struct DHTForwarder {
    output: Broker<DHTRoutingMessage>,
    tx: watch::Sender<DHTRoutingStorage>,
    ds: Box<dyn DataStorage + Send>,
}

// pub trait SubsystemTranslator<I: Async, O: Async> {
//     async fn translate_input(&mut self, trail: Vec<BrokerID>, from_broker: I);
//     async fn translate_output(&mut self, trail: Vec<BrokerID>, from_broker: O);
//     async fn settle(&mut self, callers: Vec<BrokerID>) -> Result<(), BrokerError>;
// }
#[platform_async_trait()]
impl SubsystemTranslator<DHTRoutingIn, DHTRoutingOut> for DHTForwarder {
    async fn translate_input(&mut self, trail: Vec<BrokerID>, input: DHTRoutingIn) {
        if let DHTRoutingIn::FromNetwork(src, ModuleMessage::Route(id, msg)) = input {
            self.output
                .emit_msg_dest(
                    broker::Destination::Forwarded(trail),
                    DHTRoutingMessage::Store(src, id, msg),
                )
                .expect("Sending");
        }
    }

    async fn translate_output(&mut self, trail: Vec<BrokerID>, output: DHTRoutingOut) {
        if let DHTRoutingOut::UpdateStorage(update) = output {
            self.tx.send(update.clone()).expect("updated storage");
            if let Ok(val) = update.to_yaml() {
                self.ds.set(MODULE_NAME, &val).expect("updating storage");
            }

            self.output
                .emit_msg_dest(
                    broker::Destination::Forwarded(trail),
                    DHTRoutingMessage::UpdateStorage(update),
                )
                .expect("Sending");
        }
    }

    async fn settle(&mut self, callers: Vec<BrokerID>) -> Result<(), BrokerError> {
        self.output.settle(callers).await
    }
}

#[cfg(test)]
mod tests {
    use flarch::start_logging_filter_level;

    use super::*;

    #[tokio::test]
    async fn test_increase() -> Result<(), Box<dyn Error>> {
        start_logging_filter_level(vec![], log::LevelFilter::Info);

        // let ds = Box::new(DataStorageTemp::new());
        // let id0 = NodeID::rnd();
        // let id1 = NodeID::rnd();
        // let mut rnd = Broker::new();
        // let mut tr = DHTRouting::start(ds, id0, rnd.clone(), DHTRoutingConfig::default()).await?;
        // let mut tap = rnd.get_tap().await?;
        // assert_eq!(0, tr.get_counter());

        // rnd.settle_msg(RandomMessage::Output(RandomOut::NodeIDsConnected(
        //     vec![id1].into(),
        // )))
        // .await?;
        // tr.increase_self(1)?;
        // assert!(matches!(
        //     tap.0.recv().await.unwrap(),
        //     RandomMessage::Input(_)
        // ));
        // assert_eq!(1, tr.get_counter());
        Ok(())
    }
}
