use flarch::{
    broker::SubsystemHandler, data_storage::DataStorage, nodeids::U256, platform_async_trait,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use tokio::sync::watch;

use crate::overlay::messages::{NetworkWrapper, OverlayIn, OverlayMessage, OverlayOut};
use flarch::{
    broker::{Broker, Subsystem},
    nodeids::NodeID,
};

use super::{
    core::{DHTRoutingConfig, DHTRoutingStorage, DHTRoutingStorageSave},
    messages::{
        DHTRoutingIn, DHTRoutingMessageIntern, DHTRoutingMessages, DHTRoutingOut, ModuleMessage,
    },
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
    pub overlay: Broker<OverlayMessage>,
    pub output: Broker<DHTRoutingMessage>,
    _storage: watch::Receiver<DHTRoutingStorage>,
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

        let mut dht_routing = Broker::new();
        let overlay = Broker::new();

        dht_routing
            .add_subsystem(Subsystem::Handler(Box::new(messages)))
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
        let output = Broker::new();
        dht_routing
            .add_subsystem(Subsystem::Handler(Box::new(DHTForwarder {
                output: output.clone(),
                tx,
                ds,
            })))
            .await?;

        Ok(DHTRouting {
            overlay,
            output,
            _storage: storage,
        })
    }

    fn link_net_dhtrouting(msg: OverlayMessage) -> Option<DHTRoutingMessageIntern> {
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

    fn link_dhtrouting_net(msg: DHTRoutingMessageIntern) -> Option<OverlayMessage> {
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
}

struct DHTForwarder {
    output: Broker<DHTRoutingMessage>,
    tx: watch::Sender<DHTRoutingStorage>,
    ds: Box<dyn DataStorage + Send>,
}

#[platform_async_trait()]
impl SubsystemHandler<DHTRoutingMessageIntern> for DHTForwarder {
    async fn messages(
        &mut self,
        msgs: Vec<DHTRoutingMessageIntern>,
    ) -> Vec<DHTRoutingMessageIntern> {
        for msg in msgs {
            if let DHTRoutingMessageIntern::Output(DHTRoutingOut::UpdateStorage(update)) = msg {
                self.tx.send(update.clone()).expect("updated storage");
                if let Ok(val) = update.to_yaml() {
                    self.ds.set(MODULE_NAME, &val).expect("updating storage");
                }

                self.output
                    .emit_msg(DHTRoutingMessage::UpdateStorage(update))
                    .expect("Sending");
            } else if let DHTRoutingMessageIntern::Input(DHTRoutingIn::FromNetwork(
                src,
                ModuleMessage::Route(id, msg),
            )) = msg
            {
                self.output
                    .emit_msg(DHTRoutingMessage::Store(src, id, msg))
                    .expect("Sending");
            }
        }
        vec![]
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
