use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WSError {
    #[error("In underlying system: {0}")]
    Underlying(String),
}

#[derive(Error, Debug)]
pub enum WSClientError {
    #[error("While connecting {0}")]
    Connection(String),
    #[error(transparent)]
    Broker(#[from] flmodules::broker::BrokerError),
}

#[cfg(feature = "libc")]
#[derive(Error, Debug)]
pub enum WSSError {
    #[error(transparent)]
    Broker(#[from] flmodules::broker::BrokerError),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Client(#[from] tokio_tungstenite::tungstenite::Error),
    #[error(transparent)]
    IO(#[from] std::io::Error),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum WSClientMessage {
    Output(WSClientOutput),
    Input(WSClientInput),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum WSClientInput {
    Message(String),
    Disconnect,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum WSClientOutput {
    Message(String),
    Disconnect,
    Connected,
    Error(String),
}

impl From<WSClientInput> for WSClientMessage {
    fn from(input: WSClientInput) -> Self {
        WSClientMessage::Input(input)
    }
}

impl From<WSClientOutput> for WSClientMessage {
    fn from(output: WSClientOutput) -> Self {
        WSClientMessage::Output(output)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum WSServerMessage {
    Output(WSServerOutput),
    Input(WSServerInput),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum WSServerOutput {
    Message((usize, String)),
    NewConnection(usize),
    Disconnection(usize),
    Stopped,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum WSServerInput {
    Message((usize, String)),
    Close(usize),
    Stop,
}

impl From<WSServerInput> for WSServerMessage {
    fn from(msg: WSServerInput) -> Self {
        WSServerMessage::Input(msg)
    }
}

impl From<WSServerOutput> for WSServerMessage {
    fn from(msg: WSServerOutput) -> Self {
        WSServerMessage::Output(msg)
    }
}