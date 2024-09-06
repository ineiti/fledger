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
use async_trait::async_trait;
use flmodules::broker::{Broker, Subsystem, SubsystemHandler};
use thiserror::Error;

use crate::web_rtc::messages::{
    ConnectionStateMap, DataChannelState, PeerMessage, WebRTCInput, WebRTCMessage, WebRTCOutput,
    WebRTCSpawner,
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
    Broker(#[from] flmodules::broker::BrokerError),
}

#[derive(Debug, Clone, PartialEq)]
/// Messages for the [`NodeConnection`] broker.
pub enum NCMessage {
    /// Messages to the [`crate::web_rtc::WebRTCConn`] broker
    Output(NCOutput),
    /// Messages from the [`crate::web_rtc::WebRTCConn`] broker
    Input(NCInput),
    /// Messages to/from the incoming connection
    Incoming(WebRTCMessage),
    /// Messages to/from the outgoing connection
    Outgoing(WebRTCMessage),
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
    State((Direction, ConnectionStateMap)),
    /// Setup message for the connection in the given direction
    Setup((Direction, PeerMessage)),
}

#[derive(Debug, Clone, PartialEq)]
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
    Setup((Direction, PeerMessage)),
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
    pub async fn new(spawner: &WebRTCSpawner) -> Result<Broker<NCMessage>, NCError> {
        let mut broker = Broker::new();
        broker
            .link_bi(
                spawner().await?,
                Box::new(Self::to_incoming),
                Box::new(Self::from_incoming),
            )
            .await?;
        broker
            .link_bi(
                spawner().await?,
                Box::new(Self::to_outgoing),
                Box::new(Self::from_outgoing),
            )
            .await?;
        let nc = NodeConnection {
            msg_queue: vec![],
            state_incoming: None,
            state_outgoing: None,
        };
        broker
            .add_subsystem(Subsystem::Handler(Box::new(nc)))
            .await?;
        Ok(broker)
    }

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

    fn send(&mut self, dir: Direction, state: Option<ConnectionStateMap>) -> Vec<NCMessage> {
        if let Some(csm) = state {
            if let Some(dc) = csm.data_connection {
                if dc == DataChannelState::Open {
                    return self
                        .msg_queue
                        .drain(..)
                        .map(|msg| WebRTCMessage::Input(WebRTCInput::Text(msg)))
                        .map(|msg| match dir {
                            Direction::Incoming => NCMessage::Incoming(msg),
                            Direction::Outgoing => NCMessage::Outgoing(msg),
                        })
                        .collect();
                }
            }
        }
        vec![]
    }

    // First try to send through outgoing queue, if that fails, try incoming queue.
    fn send_queue(&mut self) -> Vec<NCMessage> {
        let mut out = vec![];
        if self.msg_queue.len() > 0 {
            out.extend(self.send(Direction::Outgoing, self.state_outgoing));
        }
        if self.msg_queue.len() > 0 {
            out.extend(self.send(Direction::Incoming, self.state_incoming));
        }
        out
    }

    fn msg_in(&mut self, msg: NCInput) -> Vec<NCMessage> {
        match msg {
            NCInput::Text(msg_str) => {
                self.msg_queue.push(msg_str);
                let mut out = vec![];
                out.extend(self.send_queue());
                if self.state_outgoing.is_none() {
                    out.push(NCMessage::Outgoing(WebRTCMessage::Input(
                        WebRTCInput::Setup(PeerMessage::Init),
                    )));
                    self.state_outgoing = Some(ConnectionStateMap::default());
                }
                out
            }
            NCInput::Disconnect => vec![
                NCMessage::Incoming(WebRTCMessage::Input(WebRTCInput::Reset)),
                NCMessage::Outgoing(WebRTCMessage::Input(WebRTCInput::Reset)),
            ],

            NCInput::GetStates => {
                let mut out = vec![];
                if let Some(state) = self.state_incoming {
                    out.push(NCMessage::Output(NCOutput::State((
                        Direction::Incoming,
                        state.clone(),
                    ))));
                }
                if let Some(state) = self.state_outgoing {
                    out.push(NCMessage::Output(NCOutput::State((
                        Direction::Outgoing,
                        state.clone(),
                    ))));
                }
                out
            }
            NCInput::Setup((dir, pm)) => match dir {
                Direction::Incoming => vec![NCMessage::Incoming(WebRTCMessage::Input(
                    WebRTCInput::Setup(pm),
                ))],
                Direction::Outgoing => vec![NCMessage::Outgoing(WebRTCMessage::Input(
                    WebRTCInput::Setup(pm),
                ))],
            },
        }
    }

    fn msg_conn(&mut self, dir: Direction, msg: WebRTCMessage) -> Vec<NCMessage> {
        match msg {
            WebRTCMessage::Output(msg_out) => match msg_out {
                WebRTCOutput::Connected => {
                    let state = Some(ConnectionStateMap {
                        data_connection: Some(DataChannelState::Open),
                        ..Default::default()
                    });
                    match dir {
                        Direction::Incoming => self.state_incoming = state,
                        Direction::Outgoing => self.state_outgoing = state,
                    }
                    let mut out = vec![NCMessage::Output(NCOutput::Connected(dir))];
                    out.extend(self.send_queue());
                    out
                }
                WebRTCOutput::Setup(pm) => vec![NCMessage::Output(NCOutput::Setup((dir, pm)))],
                WebRTCOutput::Text(msg_str) => {
                    vec![NCMessage::Output(NCOutput::Text(msg_str))]
                }
                WebRTCOutput::State(state) => {
                    match dir {
                        Direction::Incoming => self.state_incoming = Some(state),
                        Direction::Outgoing => self.state_outgoing = Some(state),
                    }
                    vec![NCMessage::Output(NCOutput::State((dir, state)))]
                }
                WebRTCOutput::Disconnected | WebRTCOutput::Error(_) => {
                    let msg = match dir {
                        Direction::Incoming => {
                            self.state_incoming = None;
                            NCMessage::Incoming(WebRTCMessage::Input(WebRTCInput::Reset))
                        }
                        Direction::Outgoing => {
                            self.state_outgoing = None;
                            NCMessage::Outgoing(WebRTCMessage::Input(WebRTCInput::Reset))
                        }
                    };
                    vec![msg, NCMessage::Output(NCOutput::Disconnected(dir))]
                }
            },
            _ => vec![],
        }
    }
}

#[cfg_attr(feature = "nosend", async_trait(?Send))]
#[cfg_attr(not(feature = "nosend"), async_trait)]
impl SubsystemHandler<NCMessage> for NodeConnection {
    async fn messages(&mut self, msgs: Vec<NCMessage>) -> Vec<NCMessage> {
        let mut out = vec![];
        for msg in msgs {
            out.extend(match msg {
                NCMessage::Input(msg_in) => self.msg_in(msg_in),
                NCMessage::Incoming(msg_conn) => self.msg_conn(Direction::Incoming, msg_conn),
                NCMessage::Outgoing(msg_conn) => self.msg_conn(Direction::Outgoing, msg_conn),
                _ => vec![],
            });
        }

        out
    }
}

impl From<NCInput> for NCMessage {
    fn from(msg: NCInput) -> NCMessage {
        NCMessage::Input(msg)
    }
}

impl From<NCOutput> for NCMessage {
    fn from(msg: NCOutput) -> NCMessage {
        NCMessage::Output(msg)
    }
}
