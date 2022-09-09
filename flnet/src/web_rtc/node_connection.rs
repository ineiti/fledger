use async_trait::async_trait;
use flmodules::{broker::{Broker, Destination, Subsystem, SubsystemListener}, nodeids::NodeID};
use thiserror::Error;

use crate::{web_rtc::messages::{
    ConnectionStateMap, DataChannelState, PeerMessage, WebRTCInput, WebRTCMessage, WebRTCOutput,
    WebRTCSpawner,
}, network::NetworkMessage};

use super::WebRTCConnMessage;

#[derive(Error, Debug)]
pub enum NCError {
    #[error("Couldn't use output queue")]
    OutputQueue,
    #[error(transparent)]
    Setup(#[from] crate::web_rtc::messages::SetupError),
    #[error(transparent)]
    Broker(#[from] flmodules::broker::BrokerError),
}

#[derive(Debug, Clone, PartialEq)]
pub enum NCMessage {
    Output(NCOutput),
    Input(NCInput),
    Incoming(WebRTCMessage),
    Outgoing(WebRTCMessage),
}

#[derive(Debug, Clone, PartialEq)]
pub enum NCOutput {
    Connected(Direction),
    Disconnected(Direction),
    Text(String),
    State((Direction, ConnectionStateMap)),
    Setup((Direction, PeerMessage)),
}

#[derive(Debug, Clone, PartialEq)]
pub enum NCInput {
    Text(String),
    Disconnect,
    GetStates,
    Setup((Direction, PeerMessage)),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Direction {
    Incoming,
    Outgoing,
}

/// There might be up to two connections per remote node.
/// This is in the case both nodes try to set up a connection at the same time.
/// This race condition is very difficult to catch, so it's easier to just allow
/// two connections per remote node.
/// If a second, third, or later incoming connection from the same node happens, the previous
/// connection is considered stale and discarded.
pub struct NodeConnection {
    msg_queue: Vec<String>,
    state_incoming: Option<ConnectionStateMap>,
    state_outgoing: Option<ConnectionStateMap>,
}

impl NodeConnection {
    pub async fn new(spawner: &WebRTCSpawner) -> Result<Broker<NCMessage>, NCError> {
        let mut broker = Broker::new();
        broker
            .link_bi(
                spawner().await?,
                Box::new(Self::to_incoming),
                Box::new(Self::from_incoming),
            )
            .await;
        broker
            .link_bi(
                spawner().await?,
                Box::new(Self::to_outgoing),
                Box::new(Self::from_outgoing),
            )
            .await;
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
impl SubsystemListener<NCMessage> for NodeConnection {
    async fn messages(&mut self, msgs: Vec<NCMessage>) -> Vec<(Destination, NCMessage)> {
        let mut out = vec![];
        for msg in msgs {
            out.extend(match msg {
                NCMessage::Input(msg_in) => self.msg_in(msg_in),
                NCMessage::Incoming(msg_conn) => self.msg_conn(Direction::Incoming, msg_conn),
                NCMessage::Outgoing(msg_conn) => self.msg_conn(Direction::Outgoing, msg_conn),
                _ => vec![],
            });
        }

        out.into_iter()
            .map(|msg| (Destination::Others, msg))
            .collect()
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

impl NCInput {
    pub fn to_net(self, dst: NodeID) -> NetworkMessage {
        NetworkMessage::WebRTC(WebRTCConnMessage::InputNC((dst, self)))
    }
}
