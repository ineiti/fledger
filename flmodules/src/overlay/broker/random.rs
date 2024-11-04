use flarch::broker::{Broker, BrokerError};

use super::super::messages::{OverlayIn, OverlayMessage, OverlayOut};
use crate::random_connections::messages::{RandomIn, RandomMessage, RandomOut};

pub struct OverlayRandom {}

impl OverlayRandom {
    pub async fn start(
        random: Broker<RandomMessage>,
    ) -> Result<Broker<OverlayMessage>, BrokerError> {
        let mut b = Broker::new();
        // Translate RandomOut to OverlayOut, and OverlayIn to RandomIn.
        // A module connected to the Broker<OverlayMessage> will get translations of the
        // RandomOut messages, and can send messages to RandomIn using the Overlay.
        b.link_bi(
            random,
            Box::new(|msg| {
                if let RandomMessage::Output(out) = msg {
                    let ret = match out {
                        RandomOut::NodeIDsConnected(node_ids) => {
                            OverlayOut::NodeIDsConnected(node_ids)
                        }
                        RandomOut::NodeInfosConnected(infos) => {
                            OverlayOut::NodeInfosConnected(infos)
                        }
                        RandomOut::NetworkWrapperFromNetwork(id, module_message) => {
                            OverlayOut::NetworkWrapperFromNetwork(id, module_message)
                        }
                        _ => return None,
                    };
                    return Some(ret.into());
                }
                None
            }),
            Box::new(|msg| {
                if let OverlayMessage::Input(input) = msg {
                    let ret = match input {
                        OverlayIn::NetworkWrapperToNetwork(id, module_message) => {
                            RandomIn::NetworkWrapperToNetwork(id, module_message)
                        }
                    };
                    return Some(RandomMessage::Input(ret));
                }
                None
            }),
        )
        .await?;
        Ok(b)
    }
}
