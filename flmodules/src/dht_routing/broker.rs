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
        dht_routing
            .add_translator_link(
                net.clone(),
                Box::new(Self::bi_dht_out),
                Box::new(Self::bi_overlay_out),
            )
            .await?;
        dht_routing
            .add_translator_direct(
                overlay.clone(),
                Box::new(Self::direct_dht_in),
                Box::new(Self::direct_dht_out),
            )
            .await?;
        let output = Broker::new();
        dht_routing
            .add_translator(Box::new(DHTForwarder {
                output: output.clone(),
                tx,
                ds,
            }))
            .await?;

        Ok(DHTRouting {
            overlay,
            output,
            _storage: storage,
        })
    }

    fn bi_overlay_out(msg: OverlayOut) -> Option<DHTRoutingIn> {
        match msg {
            OverlayOut::NodeIDsConnected(list) => Some(DHTRoutingIn::UpdateNodeList(list.into())),
            OverlayOut::NetworkWrapperFromNetwork(id, msg) => msg
                .unwrap_yaml(MODULE_NAME)
                .map(|msg| DHTRoutingIn::FromNetwork(id, msg)),
            _ => None,
        }
    }

    fn bi_dht_out(msg: DHTRoutingOut) -> Option<OverlayIn> {
        match msg {
            DHTRoutingOut::ToNetwork(id, msg_node) => Some(OverlayIn::NetworkWrapperToNetwork(
                id,
                NetworkWrapper::wrap_yaml(MODULE_NAME, &msg_node).unwrap(),
            )),
            _ => None,
        }
    }

    fn direct_dht_out(msg: DHTRoutingOut) -> Option<OverlayOut> {
        match msg {
            DHTRoutingOut::FromDHTNetwork(u256, network_wrapper) => {
                Some(OverlayOut::NetworkWrapperFromNetwork(u256, network_wrapper))
            }
            DHTRoutingOut::UpdateDHTNodeList(node_ids) => {
                Some(OverlayOut::NodeIDsConnected(node_ids))
            }
            _ => None,
        }
    }

    fn direct_dht_in(msg: DHTRoutingIn) -> Option<OverlayIn> {
        match msg {
            DHTRoutingIn::ToDHTNetwork(u256, network_wrapper) => {
                Some(OverlayIn::NetworkWrapperToNetwork(u256, network_wrapper))
            }
            _ => None,
        }
    }
}

struct DHTForwarder {
    output: Broker<DHTRoutingMessage>,
    tx: watch::Sender<DHTRoutingStorage>,
    ds: Box<dyn DataStorage + Send>,
}

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
