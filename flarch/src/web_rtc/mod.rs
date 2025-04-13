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

use flmacro::platform_async_trait;
use node_connection::Direction;

use crate::{
    broker::{Broker, SubsystemHandler},
    nodeids::NodeID,
};

use self::{
    messages::WebRTCSpawner,
    node_connection::{NCInput, NCOutput, NodeConnection},
};

pub mod connection;
pub mod messages;
pub mod node_connection;
pub mod websocket;

#[cfg(target_family = "windows")]
compile_error!("flarch is not available for windows");

#[cfg(target_family = "wasm")]
mod wasm;
#[cfg(target_family = "wasm")]
pub use wasm::*;

#[cfg(target_family = "unix")]
mod libc;
#[cfg(target_family = "unix")]
pub use libc::*;

pub type BrokerWebRTCConn = Broker<WebRTCConnInput, WebRTCConnOutput>;

#[derive(Debug, Clone, PartialEq)]
pub enum WebRTCConnInput {
    Message(NodeID, NCInput),
    Connect(NodeID, Direction),
    Disconnect(NodeID),
}

#[derive(Debug, Clone, PartialEq)]
pub enum WebRTCConnOutput {
    Message(NodeID, NCOutput),
}

/// The actual implementation of the WebRTC connection setup.
pub struct WebRTCConn {
    web_rtc: WebRTCSpawner,
    connections: HashMap<NodeID, Broker<NCInput, NCOutput>>,
    broker: BrokerWebRTCConn,
}

impl WebRTCConn {
    /// Creates a new [`Broker<WebRTCConnMessage>`] that will accept incoming connections and set up
    /// new outgoing connections.
    pub async fn new(web_rtc: WebRTCSpawner) -> anyhow::Result<BrokerWebRTCConn> {
        let mut br = Broker::new();
        br.add_handler(Box::new(Self {
            web_rtc,
            connections: HashMap::new(),
            broker: br.clone(),
        }))
        .await?;
        Ok(br)
    }

    /// Ensures that a given connection exists.
    async fn ensure_connection(&mut self, id: NodeID, dir: Direction) -> anyhow::Result<()> {
        if !self.connections.contains_key(&id) {
            let mut nc = NodeConnection::new(&self.web_rtc).await?;
            self.connections.insert(id.clone(), nc.clone());
            nc.add_translator_o_to(
                self.broker.clone(),
                Box::new(move |msg| Some(WebRTCConnOutput::Message(id.clone(), msg))),
            )
            .await?;
            if dir == Direction::Outgoing {
                self.try_send(
                    id,
                    NCInput::Setup(Direction::Outgoing, messages::PeerMessage::Init),
                );
            }
        }

        Ok(())
    }

    fn try_send(&mut self, dst: NodeID, msg: NCInput) {
        if let Some(conn) = self.connections.get_mut(&dst) {
            conn.emit_msg_in(msg.clone())
                .err()
                .map(|e| log::error!("When sending message {msg:?} to webrtc: {e:?}"));
        } else {
            log::warn!("Dropping message {:?} to unconnected node {}", msg, dst);
        }
    }
}

#[platform_async_trait()]
impl SubsystemHandler<WebRTCConnInput, WebRTCConnOutput> for WebRTCConn {
    async fn messages(&mut self, msgs: Vec<WebRTCConnInput>) -> Vec<WebRTCConnOutput> {
        for msg in msgs {
            match msg {
                WebRTCConnInput::Message(dst, msg_in) => {
                    self.try_send(dst, msg_in);
                }
                WebRTCConnInput::Disconnect(dst) => {
                    self.try_send(dst, NCInput::Disconnect);
                }
                WebRTCConnInput::Connect(dst, dir) => {
                    self.ensure_connection(dst.clone(), dir)
                        .await
                        .err()
                        .map(|e| log::error!("When starting webrtc-connection {e:?}"));
                }
            };
        }
        vec![]
    }
}
