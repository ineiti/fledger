use std::collections::HashMap;

use async_trait::async_trait;

use flmodules::{
    broker::{Broker, BrokerError, SubsystemListener, Translate},
    nodeids::{NodeID, U256},
};

use self::{
    messages::WebRTCSpawner,
    node_connection::{NCError, NCInput, NCMessage, NCOutput, NodeConnection},
};

pub mod messages;
pub mod node_connection;

#[derive(Debug, Clone, PartialEq)]
pub enum WebRTCConnMessage {
    InputNC((U256, NCInput)),
    OutputNC((U256, NCOutput)),
    Connect(U256),
}

pub struct WebRTCConn {
    web_rtc: WebRTCSpawner,
    connections: HashMap<NodeID, Broker<NCMessage>>,
    broker: Broker<WebRTCConnMessage>,
}

impl WebRTCConn {
    pub async fn new(web_rtc: WebRTCSpawner) -> Result<Broker<WebRTCConnMessage>, BrokerError> {
        let mut br = Broker::new();
        br.add_subsystem(flmodules::broker::Subsystem::Handler(Box::new(Self {
            web_rtc,
            connections: HashMap::new(),
            broker: br.clone(),
        })))
        .await?;
        Ok(br)
    }

    /// Ensures that a given connection exists.
    async fn ensure_connection(&mut self, id: &U256) -> Result<(), NCError> {
        if !self.connections.contains_key(id) {
            let mut nc = NodeConnection::new(&self.web_rtc).await?;
            nc.forward(self.broker.clone(), Self::from_nc(id.clone()))
                .await;
            self.connections.insert(*id, nc);
        }

        Ok(())
    }

    fn from_nc(id: U256) -> Translate<NCMessage, WebRTCConnMessage> {
        Box::new(move |msg| {
            if let NCMessage::Output(ncmsg) = msg {
                return Some(WebRTCConnMessage::OutputNC((id, ncmsg)));
            }
            None
        })
    }
}

#[cfg_attr(feature = "nosend", async_trait(?Send))]
#[cfg_attr(not(feature = "nosend"), async_trait)]
impl SubsystemListener<WebRTCConnMessage> for WebRTCConn {
    async fn messages(&mut self, msgs: Vec<WebRTCConnMessage>) -> Vec<WebRTCConnMessage> {
        for msg in msgs {
            match msg {
                WebRTCConnMessage::InputNC((dst, msg_in)) => {
                    if let Some(conn) = self.connections.get_mut(&dst) {
                        conn.emit_msg(NCMessage::Input(msg_in.clone()))
                            .err()
                            .map(|e| {
                                log::error!("When sending message {msg_in:?} to webrtc: {e:?}")
                            });
                    } else {
                        log::warn!("Dropping message {:?} to unconnected node {}", msg_in, dst);
                    }
                }
                WebRTCConnMessage::Connect(dst) => {
                    self.ensure_connection(&dst)
                        .await
                        .err()
                        .map(|e| log::error!("When starting webrtc-connection {e:?}"));
                }
                _ => {}
            };
        }
        vec![]
    }
}
