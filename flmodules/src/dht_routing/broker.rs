use flarch::{data_storage::DataStorage, platform_async_trait};
use std::error::Error;
use tokio::sync::watch;

use crate::overlay::messages::{NetworkWrapper, OverlayIn, OverlayMessage, OverlayOut};
use flarch::{
    broker::{Broker, Subsystem, SubsystemHandler},
    nodeids::NodeID,
};

use super::{
    core::{DHTRoutingConfig, DHTRoutingStorage, DHTRoutingStorageSave},
    messages::{DHTRoutingIn, DHTRoutingMessage, DHTRoutingMessages, DHTRoutingOut},
};

const MODULE_NAME: &str = "DHTRouting";

/// This links the DHTRouting module with other modules, so that
/// all messages are correctly translated from one to the other.
/// For this example, it uses the RandomConnections module to communicate
/// with other nodes.
///
/// The [DHTRouting] holds the [Translate] and offers convenience methods
/// to interact with [Translate] and [DHTRoutingMessage].
pub struct DHTRouting {
    pub overlay: Broker<OverlayMessage>,
    storage: watch::Receiver<DHTRoutingStorage>,
}

impl DHTRouting {
    pub async fn start(
        ds: Box<dyn DataStorage + Send>,
        our_id: NodeID,
        net: Broker<OverlayMessage>,
        config: DHTRoutingConfig,
    ) -> Result<Self, Box<dyn Error>> {
        let str = ds.get(MODULE_NAME).unwrap_or("".into());
        let storage = DHTRoutingStorageSave::from_str(&str).unwrap_or_default();
        let messages = DHTRoutingMessages::new(storage.clone(), config, our_id)?;
        let (tx, storage) = watch::channel(storage);

        Ok(DHTRouting {
            overlay: Translate::start(ds, tx, net, messages).await?,
            storage,
        })
    }

    pub fn get_counter(&self) -> u32 {
        self.storage.borrow().counter
    }
}

/// Translates the messages to/from the RandomMessage and calls `DHTRoutingMessages.processMessages`.
struct Translate {
    ds: Box<dyn DataStorage + Send>,
    tx: watch::Sender<DHTRoutingStorage>,
    messages: DHTRoutingMessages,
}

impl Translate {
    async fn start(
        ds: Box<dyn DataStorage + Send>,
        tx: watch::Sender<DHTRoutingStorage>,
        net: Broker<OverlayMessage>,
        messages: DHTRoutingMessages,
    ) -> Result<Broker<OverlayMessage>, Box<dyn Error>> {
        let mut dht_routing = Broker::new();
        let overlay = Broker::new();

        dht_routing
            .add_subsystem(Subsystem::Handler(Box::new(Translate { ds, tx, messages })))
            .await?;
        dht_routing
            .link_bi(
                net,
                Box::new(Self::link_net_dhtrouting),
                Box::new(Self::link_dhtrouting_net),
            )
            .await?;
        dht_routing
            .link_bi(
                overlay.clone(),
                Box::new(Self::link_overlay_dhtrouting),
                Box::new(Self::link_dhtrouting_overlay),
            )
            .await?;
        Ok(overlay)
    }

    fn link_net_dhtrouting(msg: OverlayMessage) -> Option<DHTRoutingMessage> {
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

    fn link_dhtrouting_net(msg: DHTRoutingMessage) -> Option<OverlayMessage> {
        if let DHTRoutingMessage::Output(DHTRoutingOut::ToNetwork(id, msg_node)) = msg {
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

    fn link_overlay_dhtrouting(msg: OverlayMessage) -> Option<DHTRoutingMessage> {
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

    fn link_dhtrouting_overlay(msg: DHTRoutingMessage) -> Option<OverlayMessage> {
        if let DHTRoutingMessage::Output(DHTRoutingOut::ToNetwork(id, msg_node)) = msg {
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
}

#[platform_async_trait()]
impl SubsystemHandler<DHTRoutingMessage> for Translate {
    async fn messages(&mut self, msgs: Vec<DHTRoutingMessage>) -> Vec<DHTRoutingMessage> {
        let mut msgs_in = vec![];
        for msg in msgs {
            match msg {
                DHTRoutingMessage::Input(msg_in) => msgs_in.push(msg_in),
                DHTRoutingMessage::Output(DHTRoutingOut::UpdateStorage(sto)) => {
                    self.tx.send(sto.clone()).expect("updated storage");
                    if let Ok(val) = sto.to_yaml() {
                        self.ds.set(MODULE_NAME, &val).expect("updating storage");
                    }
                }
                _ => {}
            }
        }
        self.messages
            .process_messages(msgs_in)
            .into_iter()
            .map(|o| o.into())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use flarch::{data_storage::DataStorageTemp, start_logging_filter_level};

    use super::*;

    #[tokio::test]
    async fn test_increase() -> Result<(), Box<dyn Error>> {
        start_logging_filter_level(vec![], log::LevelFilter::Info);

        let ds = Box::new(DataStorageTemp::new());
        let id0 = NodeID::rnd();
        let id1 = NodeID::rnd();
        let mut rnd = Broker::new();
        let mut tr = DHTRouting::start(ds, id0, rnd.clone(), DHTRoutingConfig::default()).await?;
        let mut tap = rnd.get_tap().await?;
        assert_eq!(0, tr.get_counter());

        // rnd.settle_msg(RandomMessage::Output(RandomOut::NodeIDsConnected(
        //     vec![id1].into(),
        // )))
        // .await?;
        // tr.increase_self(1)?;
        // assert!(matches!(
        //     tap.0.recv().await.unwrap(),
        //     RandomMessage::Input(_)
        // ));
        assert_eq!(1, tr.get_counter());
        Ok(())
    }
}
