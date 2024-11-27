use flarch::{
    broker_io::BrokerIO, data_storage::DataStorage, platform_async_trait, tasks::spawn_local,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use tokio::sync::watch;

use crate::{
    dht_routing::broker::{DHTRoutingIn, DHTRoutingMessage, DHTRoutingOut},
    flo::dht::DHTStorageConfig,
    overlay::messages::{NetworkWrapper, OverlayIn, OverlayOut},
    random_connections::messages::{RandomIn, RandomMessage, RandomOut},
};
use flarch::nodeids::NodeID;

use super::{
    core::{DHTStorageBucket, DHTStorageStorageSave},
    messages::{DHTStorageMessages, InternIn, InternOut},
};

pub(super) const MODULE_NAME: &str = "DHTStorage";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DHTStorageIn {}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DHTStorageOut {}

/// This links the DHTStorage module with other modules, so that
/// all messages are correctly translated from one to the other.
/// For this example, it uses the RandomConnections module to communicate
/// with other nodes.
///
/// The [DHTStorage] holds the [Translate] and offers convenience methods
/// to interact with [Translate] and [DHTStorageMessage].
pub struct DHTStorage {
    /// Represents the underlying broker.
    pub broker: BrokerIO<DHTStorageIn, DHTStorageOut>,
    our_id: NodeID,
    storage: watch::Receiver<DHTStorageBucket>,
}

impl DHTStorage {
    pub async fn start(
        mut ds: Box<dyn DataStorage + Send>,
        our_id: NodeID,
        net: BrokerIO<OverlayIn, OverlayOut>,
        dht_routing: BrokerIO<DHTRoutingIn, DHTRoutingOut>,
        config: DHTStorageConfig,
    ) -> Result<Self, Box<dyn Error>> {
        let str = ds.get(MODULE_NAME).unwrap_or("".into());
        let storage = DHTStorageStorageSave::from_str(&str).unwrap_or_default();
        let messages = DHTStorageMessages::new(storage.clone(), config, our_id)?;
        let (tx, storage) = watch::channel(storage);

        let mut dht_storage = BrokerIO::<InternIn, InternOut>::new();

        dht_storage.add_handler(Box::new(messages)).await?;
        // dht_storage
        //     .add_translator_link(
        //         net.clone(),
        //         Box::new(Self::bi_dht_out),
        //         Box::new(Self::bi_overlay_out),
        //     )
        //     .await?;
        // dht_storage
        //     .add_translator_direct(
        //         overlay.clone(),
        //         Box::new(Self::direct_dht_in),
        //         Box::new(Self::direct_dht_out),
        //     )
        //     .await?;
        // let output = Broker::new();
        // dht_storage
        //     .add_translator(Box::new(DHTForwarder {
        //         output: output.clone(),
        //         tx,
        //         ds,
        //     }))
        //     .await?;
        // let str = ds.get(MODULE_NAME).unwrap_or("".into());
        // let storage = DHTStorageStorageSave::from_str(&str).unwrap_or_default();
        // let messages = DHTStorageMessages::new(storage.clone(), config, our_id)?;
        // let mut broker = Translate::start(rc, messages).await?;

        // let (tx, storage) = watch::channel(storage);
        // let (mut tap, _) = broker.get_tap().await?;
        // spawn_local(async move {
        //     loop {
        //         if let Some(DHTStorageMessage::Output(DHTStorageOut::UpdateStorage(sto))) =
        //             tap.recv().await
        //         {
        //             tx.send(sto.clone()).expect("updated storage");
        //             if let Ok(val) = sto.to_yaml() {
        //                 ds.set(MODULE_NAME, &val).expect("updating storage");
        //             }
        //         }
        //     }
        // });
        Ok(DHTStorage {
            broker: BrokerIO::new(),
            our_id,
            storage,
        })
    }

    // fn link_net_dhtstorage(msg: OverlayOut) -> Option<DHTStorageMessage> {
    //     if let RandomMessage::Output(msg_out) = msg {
    //         match msg_out {
    //             RandomOut::NodeIDsConnected(list) => {
    //                 Some(DHTStorageIn::UpdateNodeList(list.into()).into())
    //             }
    //             RandomOut::NetworkWrapperFromNetwork(id, msg) => msg
    //                 .unwrap_yaml(MODULE_NAME)
    //                 .map(|msg| DHTStorageIn::Node(id, msg).into()),
    //             _ => None,
    //         }
    //     } else {
    //         None
    //     }
    // }

    // fn link_dhtstorage_net(msg: DHTStorageMessage) -> Option<RandomMessage> {
    //     if let DHTStorageMessage::Output(DHTStorageOut::Node(id, msg_node)) = msg {
    //         Some(
    //             RandomIn::NetworkWrapperToNetwork(
    //                 id,
    //                 NetworkWrapper::wrap_yaml(MODULE_NAME, &msg_node).unwrap(),
    //             )
    //             .into(),
    //         )
    //     } else {
    //         None
    //     }
    // }
}

// /// Translates the messages to/from the RandomMessage and calls `DHTStorageMessages.processMessages`.
// struct Translate {
//     messages: DHTStorageMessages,
// }

// impl Translate {
//     async fn start(
//         random: Broker<RandomMessage>,
//         messages: DHTStorageMessages,
//     ) -> Result<Broker<DHTStorageMessage>, Box<dyn Error>> {
//         let mut dht_storage = Broker::new();

//         dht_storage
//             .add_subsystem(Subsystem::Handler(Box::new(Translate { messages })))
//             .await?;
//         dht_storage
//             .link_bi(
//                 random,
//                 Box::new(Self::link_rnd_dhtstorage),
//                 Box::new(Self::link_dhtstorage_rnd),
//             )
//             .await?;
//         Ok(dht_storage)
//     }
// }

// #[platform_async_trait()]
// impl SubsystemHandler<DHTStorageMessage> for Translate {
//     async fn messages(&mut self, msgs: Vec<DHTStorageMessage>) -> Vec<DHTStorageMessage> {
//         let msgs_in = msgs
//             .into_iter()
//             .filter_map(|msg| match msg {
//                 DHTStorageMessage::Input(msg_in) => Some(msg_in),
//                 DHTStorageMessage::Output(_) => None,
//             })
//             .collect();
//         self.messages
//             .process_messages(msgs_in)
//             .into_iter()
//             .map(|o| o.into())
//             .collect()
//     }
// }

#[cfg(test)]
mod tests {
    // use flarch::{data_storage::DataStorageTemp, start_logging_filter_level};

    // use super::*;
}
