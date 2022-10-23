use std::time::Duration;

use flarch::Interval;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_stream::StreamExt;

use crate::network_broker::{NetCall, NetReply, NetworkError, NetworkMessage};
use flmodules::broker::{Broker, BrokerError};

pub struct Network {
    broker_net: Broker<NetworkMessage>,
    tap: UnboundedReceiver<NetworkMessage>,
}

impl Network {
    pub async fn start(mut broker_net: Broker<NetworkMessage>) -> Result<Self, NetworkError> {
        let (mut tap, _) = broker_net.get_tap_async().await?;
        let mut timeout = Interval::new_interval(Duration::from_secs(10));
        timeout.next().await;
        loop {
            tokio::select! {
                _ = timeout.next() => {
                    return Err(NetworkError::SignallingServer);
                }
                msg = tap.recv() => {
                    if matches!(msg, Some(NetworkMessage::Reply(NetReply::RcvWSUpdateList(_)))){
                        break;
                    }
                }
            }
        }
        Ok(Self { broker_net, tap })
    }

    pub async fn recv(&mut self) -> NetReply {
        loop {
            let msg = self.tap.recv().await;
            if let Some(NetworkMessage::Reply(msg_reply)) = msg {
                return msg_reply;
            }
        }
    }

    pub fn send(&mut self, msg: NetCall) -> Result<(), BrokerError> {
        self.broker_net.emit_msg(NetworkMessage::Call(msg))
    }

    pub fn send_msg(&mut self, dst: crate::NodeID, msg: String) -> Result<(), BrokerError> {
        self.send(NetCall::SendNodeMessage(dst, msg))
    }

    pub fn send_list_request(&mut self) -> Result<(), BrokerError> {
        self.send(NetCall::SendWSUpdateListRequest)
    }
}
