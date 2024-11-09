use flarch::broker::{Broker, BrokerError};

use crate::loopix::messages::{LoopixIn, LoopixMessage, LoopixOut};

use super::super::messages::{OverlayIn, OverlayMessage, OverlayOut};

/**
 * This uses the Loopix-module to offer an anonymous and privacy-preserving
 * communication through the nodes.
 */
pub struct OverlayLoopix {
    pub broker: Broker<OverlayMessage>,
}

impl OverlayLoopix {
    pub async fn start(
        loopix: Broker<LoopixMessage>,
    ) -> Result<Broker<OverlayMessage>, BrokerError> {
        let mut broker = Broker::new();

        broker
            .link_bi(
                loopix.clone(),
                Box::new(Self::from_loopix),
                Box::new(Self::to_loopix),
            )
            .await?;

        Ok(broker)
    }

    fn from_loopix(msg: LoopixMessage) -> Option<OverlayMessage> {
        if let LoopixMessage::Output(out) = msg {
            let ret = match out {
                LoopixOut::NodeIDsConnected(node_ids) => OverlayOut::NodeIDsConnected(node_ids),
                LoopixOut::NodeInfosConnected(infos) => OverlayOut::NodeInfosConnected(infos),
                LoopixOut::NodeInfoAvailable(availables) => {
                    OverlayOut::NodeInfoAvailable(availables)
                }
                LoopixOut::OverlayReply(node_id, module_msg) => {
                    OverlayOut::NetworkWrapperFromNetwork(node_id, module_msg)
                }
                _ => return None,
            };
            return Some(ret.into());
        }
        None
    }

    fn to_loopix(msg: OverlayMessage) -> Option<LoopixMessage> {
        if let OverlayMessage::Input(input) = msg {
            match input {
                OverlayIn::NetworkWrapperToNetwork(node_id, wrapper) => {
                    return Some(LoopixIn::OverlayRequest(node_id, wrapper).into());
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod test {
    use flarch::nodeids::NodeID;

    use crate::nodeconfig::NodeInfo;

    use super::*;

    fn _check_msgs(msgs: Vec<OverlayMessage>, available: &[NodeInfo], connected: &[NodeInfo]) {
        assert_eq!(3, msgs.len());
        assert_eq!(
            OverlayMessage::Output(OverlayOut::NodeInfoAvailable(available.to_vec())),
            msgs[0]
        );
        assert_eq!(
            OverlayMessage::Output(OverlayOut::NodeIDsConnected(
                connected
                    .iter()
                    .map(|info| info.get_id())
                    .collect::<Vec<NodeID>>()
                    .into()
            )),
            msgs[1]
        );
        assert_eq!(
            OverlayMessage::Output(OverlayOut::NodeInfosConnected(connected.to_vec())),
            msgs[2]
        );
    }
}
