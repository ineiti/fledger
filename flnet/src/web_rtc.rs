use async_trait::async_trait;

use flmodules::{
    broker::{Broker, BrokerError, Destination, SubsystemListener, Translate},
    nodeids::U256,
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
    connections: Vec<U256>,
    broker: Broker<WebRTCConnMessage>,
}

impl WebRTCConn {
    pub async fn new(web_rtc: WebRTCSpawner) -> Result<Broker<WebRTCConnMessage>, BrokerError> {
        let mut br = Broker::new();
        br.add_subsystem(flmodules::broker::Subsystem::Handler(Box::new(Self {
            web_rtc,
            connections: vec![],
            broker: br.clone(),
        })))
        .await?;
        Ok(br)
    }

    /// Ensures that a given connection exists.
    async fn ensure_connection(&mut self, id: &U256) -> Result<(), NCError> {
        if !self.connections.contains(id) {
            let nc = NodeConnection::new(&self.web_rtc).await?;
            self.broker
                .link_bi(
                    nc.clone(),
                    Self::from_nc(id.clone()),
                    Self::to_nc(id.clone()),
                )
                .await;
            self.connections.push(*id);
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

    fn to_nc(id: U256) -> Translate<WebRTCConnMessage, NCMessage> {
        Box::new(move |msg| {
            if let WebRTCConnMessage::InputNC((dst, msg_nc)) = msg {
                if id == dst {
                    return Some(NCMessage::Input(msg_nc));
                }
            }
            None
        })
    }
}

#[cfg_attr(feature = "nosend", async_trait(?Send))]
#[cfg_attr(not(feature = "nosend"), async_trait)]
impl SubsystemListener<WebRTCConnMessage> for WebRTCConn {
    async fn messages(
        &mut self,
        msgs: Vec<WebRTCConnMessage>,
    ) -> Vec<(Destination, WebRTCConnMessage)> {
        let mut out = vec![];
        for msg in msgs {
            match msg {
                WebRTCConnMessage::InputNC(msg_in) => out.extend(vec![
                    WebRTCConnMessage::Connect(msg_in.0.clone()),
                    WebRTCConnMessage::InputNC(msg_in),
                ]),
                WebRTCConnMessage::Connect(dst) => {
                    self.ensure_connection(&dst)
                        .await
                        .err()
                        .map(|e| log::error!("When starting webrtc-connection {e:?}"));
                }
                _ => {}
            };
        }
        out.into_iter().map(|msg| (Destination::All, msg)).collect()
    }
}
