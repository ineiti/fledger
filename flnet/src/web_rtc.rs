//! # WebRTC connection setup
//!
//! The WebRTC connections need a lot of detail to be set up correctly.
//! Moreover, while the WebRTC protocol is very clear about how to set up
//! a connection, it doesn't describe how the nodes need to communicate to
//! set up the connection.
//! The messages for setting up the connection are described in the [`crate::signal::SignalServer`].
//! 
//! # Bidirectional connections
//! 
//! Every WebRTC connection has one or two connections inside:
//! - _outgoing_ is the connection initiated by this node
//! - _incoming_ is the connection initiated by the other node
//! 
//! This is necessary, as it is always possible that two nodes want to start
//! connecting to each other concurrently.

use std::collections::HashMap;

use async_trait::async_trait;

use flmodules::{
    broker::{Broker, BrokerError, SubsystemHandler, Translate},
    nodeids::NodeID,
};

use self::{
    messages::WebRTCSpawner,
    node_connection::{NCError, NCInput, NCMessage, NCOutput, NodeConnection},
};

pub mod messages;
pub mod node_connection;

#[derive(Debug, Clone, PartialEq)]
/// All messages for the [`WebRTCConn`] broker.
pub enum WebRTCConnMessage {
    /// Messages coming from the [`crate::network::NetworkBroker`]
    InputNC((NodeID, NCInput)),
    /// Messages going to the [`crate::network::NetworkBroker`]
    OutputNC((NodeID, NCOutput)),
    /// Connection request
    Connect(NodeID),
}

/// The actual implementation of the WebRTC connection setup.
pub struct WebRTCConn {
    web_rtc: WebRTCSpawner,
    connections: HashMap<NodeID, Broker<NCMessage>>,
    broker: Broker<WebRTCConnMessage>,
}

impl WebRTCConn {
    /// Creates a new [`Broker<WebRTCConnMessage>`] that will accept incoming connections and set up 
    /// new outgoing connections.
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
    async fn ensure_connection(&mut self, id: &NodeID) -> Result<(), NCError> {
        if !self.connections.contains_key(id) {
            let mut nc = NodeConnection::new(&self.web_rtc).await?;
            nc.forward(self.broker.clone(), Self::from_nc(id.clone()))
                .await;
            self.connections.insert(*id, nc);
        }

        Ok(())
    }

    fn from_nc(id: NodeID) -> Translate<NCMessage, WebRTCConnMessage> {
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
impl SubsystemHandler<WebRTCConnMessage> for WebRTCConn {
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
