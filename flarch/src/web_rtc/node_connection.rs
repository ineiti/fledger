//! # A single WebRTC connection
//!
//! This broker handles two connections at the same time: one outgoing connection
//! and one ingoing connection.
//! When a message is sent, it is sent to the connection that is alread up and running.
//! But if the outgoing connection is not established yet, it will be started.
//!
//! The actual calls to the WebRTC code have been abstracted, so that the same
//! code works for the libc and the wasm implementation.
//!
//! _If you come here, I hope you're not trying to debug something that doesn't work.
//! This code is quite obscure, and should be rewritten for the 5th time or so._
use crate::broker::{Broker, BrokerError, SubsystemHandler};
use enum_display::EnumDisplay;
use flmacro::platform_async_trait;
use thiserror::Error;

use crate::web_rtc::messages::{
    ConnectionStateMap, DataChannelState, PeerMessage, WebRTCInput, WebRTCOutput, WebRTCSpawner,
};

#[derive(Error, Debug)]
/// An error by the connection.
pub enum NCError {
    /// Something went wrong when sending to the output queue.
    #[error("Couldn't use output queue")]
    OutputQueue,
    /// Couldn't setup the WebRTC connection
    #[error(transparent)]
    Setup(#[from] crate::web_rtc::messages::SetupError),
    /// The broker went nuts
    #[error(transparent)]
    Broker(#[from] BrokerError),
}

#[derive(Debug, Clone, PartialEq, EnumDisplay)]
/// Messages from the [`crate::web_rtc::WebRTCConn`]
pub enum NCInput {
    /// Text to be sent over the first available connection
    Text(String),
    /// Disconnect all connections
    Disconnect,
    /// Return all states
    GetStates,
    /// Treat the [`PeerMessage`] to setup a new connection with the
    /// given direction
    Setup(Direction, PeerMessage),
    /// Messages to the incoming connection
    Incoming(WebRTCOutput),
    /// Messages to the outgoing connection
    Outgoing(WebRTCOutput),
}

#[derive(Debug, Clone, PartialEq)]
/// Messages to the [`crate::web_rtc::WebRTCConn`]
pub enum NCOutput {
    /// Created a connection in the given direction
    Connected(Direction),
    /// Connection in the given direction has been dropped
    Disconnected(Direction),
    /// Received a text from any connection
    Text(String),
    /// Return a changed state from one of the connections
    State(Direction, ConnectionStateMap),
    /// Setup message for the connection in the given direction
    Setup(Direction, PeerMessage),
    /// Messages from the incoming connection
    Incoming(WebRTCInput),
    /// Messages from the outgoing connection
    Outgoing(WebRTCInput),
}

#[derive(Debug, Clone, PartialEq)]
/// One of the directions for a connection
pub enum Direction {
    /// Being initiated by the remote peer
    Incoming,
    /// Being initiated by this peer
    Outgoing,
}

/// A single connection between two nodes, either incoming our outgoing.
/// It will do its best to detect when a connection has gone stale and shut
/// itself down.
pub struct NodeConnection {
    msg_queue: Vec<String>,
    state_incoming: Option<ConnectionStateMap>,
    state_outgoing: Option<ConnectionStateMap>,
}

impl NodeConnection {
    /// Create a new [`NodeConnection`] that will wait for a first message before
    /// setting up an outgoing connection.
    pub async fn new(spawner: &WebRTCSpawner) -> anyhow::Result<Broker<NCInput, NCOutput>> {
        let mut broker = Broker::new();
        broker
            .add_translator_link(
                spawner().await?,
                Box::new(|msg| match msg {
                    NCOutput::Incoming(msg_webrtc) => Some(msg_webrtc),
                    _ => None,
                }),
                Box::new(|msg_webrtc| Some(NCInput::Incoming(msg_webrtc))),
            )
            .await?;

        broker
            .add_translator_link(
                spawner().await?,
                Box::new(|msg| match msg {
                    NCOutput::Outgoing(msg_webrtc) => Some(msg_webrtc),
                    _ => None,
                }),
                Box::new(|msg_webrtc| Some(NCInput::Outgoing(msg_webrtc))),
            )
            .await?;

        let nc = NodeConnection {
            msg_queue: vec![],
            state_incoming: None,
            state_outgoing: None,
        };
        broker.add_handler(Box::new(nc)).await?;
        Ok(broker)
    }

    fn send(&mut self, dir: Direction, state: Option<ConnectionStateMap>) -> Vec<NCOutput> {
        if let Some(csm) = state {
            if let Some(dc) = csm.data_connection {
                if dc == DataChannelState::Open {
                    return self
                        .msg_queue
                        .drain(..)
                        .map(|msg| WebRTCInput::Text(msg))
                        .map(|msg| match dir {
                            Direction::Incoming => NCOutput::Incoming(msg),
                            Direction::Outgoing => NCOutput::Outgoing(msg),
                        })
                        .collect();
                }
            }
        }
        vec![]
    }

    // First try to send through outgoing queue, if that fails, try incoming queue.
    fn send_queue(&mut self) -> Vec<NCOutput> {
        let mut out = vec![];
        if self.msg_queue.len() > 0 {
            out.extend(self.send(Direction::Outgoing, self.state_outgoing));
        }
        if self.msg_queue.len() > 0 {
            out.extend(self.send(Direction::Incoming, self.state_incoming));
        }
        out
    }

    fn msg_in(&mut self, msg: NCInput) -> Vec<NCOutput> {
        match msg {
            NCInput::Text(msg_str) => {
                self.msg_queue.push(msg_str);
                self.send_queue()
            }
            NCInput::Disconnect => vec![
                NCOutput::Incoming(WebRTCInput::Disconnect),
                NCOutput::Outgoing(WebRTCInput::Disconnect),
            ],

            NCInput::GetStates => {
                let mut out = vec![];
                if let Some(state) = self.state_incoming {
                    out.push(NCOutput::State(Direction::Incoming, state.clone()));
                }
                if let Some(state) = self.state_outgoing {
                    out.push(NCOutput::State(Direction::Outgoing, state.clone()));
                }
                out
            }
            NCInput::Setup(dir, pm) => {
                match dir {
                    Direction::Incoming => vec![NCOutput::Incoming(WebRTCInput::Setup(pm))],
                    Direction::Outgoing => {
                        if self.state_outgoing.is_none() {
                            self.state_outgoing = Some(ConnectionStateMap::default());
                        }
                        vec![NCOutput::Outgoing(WebRTCInput::Setup(pm))]
                    }
                }
            }
            _ => vec![],
        }
    }

    fn msg_conn(&mut self, dir: Direction, msg: WebRTCOutput) -> Vec<NCOutput> {
        match msg {
            WebRTCOutput::Connected => {
                let state = Some(ConnectionStateMap {
                    data_connection: Some(DataChannelState::Open),
                    ..Default::default()
                });
                match dir {
                    Direction::Incoming => self.state_incoming = state,
                    Direction::Outgoing => self.state_outgoing = state,
                }
                let mut out = vec![NCOutput::Connected(dir)];
                out.extend(self.send_queue());
                out
            }
            WebRTCOutput::Setup(pm) => vec![NCOutput::Setup(dir, pm)],
            WebRTCOutput::Text(msg_str) => {
                vec![NCOutput::Text(msg_str)]
            }
            WebRTCOutput::State(state) => {
                match dir {
                    Direction::Incoming => self.state_incoming = Some(state),
                    Direction::Outgoing => self.state_outgoing = Some(state),
                }
                vec![NCOutput::State(dir, state)]
            }
            WebRTCOutput::Disconnected | WebRTCOutput::Error(_) => {
                let msg = match dir {
                    Direction::Incoming => {
                        self.state_incoming = None;
                        NCOutput::Incoming(WebRTCInput::Reset)
                    }
                    Direction::Outgoing => {
                        self.state_outgoing = None;
                        NCOutput::Outgoing(WebRTCInput::Reset)
                    }
                };
                vec![msg, NCOutput::Disconnected(dir)]
            }
        }
    }
}

#[platform_async_trait()]
impl SubsystemHandler<NCInput, NCOutput> for NodeConnection {
    async fn messages(&mut self, msgs: Vec<NCInput>) -> Vec<NCOutput> {
        let mut out = vec![];
        for msg in msgs {
            out.extend(match msg {
                NCInput::Incoming(msg_conn) => self.msg_conn(Direction::Incoming, msg_conn),
                NCInput::Outgoing(msg_conn) => self.msg_conn(Direction::Outgoing, msg_conn),
                _ => self.msg_in(msg),
            });
        }

        out
    }
}

/*
 fn to_incoming(msg: WebRTCMessage) -> Option<NCMessage> {
    matches!(msg, WebRTCMessage::Output(_)).then(|| (NCMessage::Incoming(msg)))
}

fn from_incoming(msg: NCMessage) -> Option<WebRTCMessage> {
    match msg {
        NCMessage::Incoming(msg) => matches!(msg, WebRTCMessage::Input(_)).then(|| (msg)),
        _ => None,
    }
}

fn to_outgoing(msg: WebRTCMessage) -> Option<NCMessage> {
    matches!(msg, WebRTCMessage::Output(_)).then(|| (NCMessage::Outgoing(msg)))
}

fn from_outgoing(msg: NCMessage) -> Option<WebRTCMessage> {
    match msg {
        NCMessage::Outgoing(msg) => matches!(msg, WebRTCMessage::Input(_)).then(|| (msg)),
        _ => None,
    }
}

    I: TranslateFrom<TO>,
    O: TranslateInto<TI>,
*/
