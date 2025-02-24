//! # Websocket structures for the broker implementations
//! 
//! These are the structures used by the [`crate::broker::BrokerWSClient`] and 
//! [`crate::broker::BrokerWSServer`].
//! The wasm and libc implementations will return one of these brokers and then
//! interpret the messages received.
//! This is similar to a trait that is implemented in either wasm or libc, but
//! allows to connect the broker directly to other brokers.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::broker::{Broker, BrokerError};

#[derive(Error, Debug)]
/// Wrapper for an error in the websocket implementations
pub enum WSError {
    /// Error from below
    #[error("In underlying system: {0}")]
    Underlying(String),
}

#[derive(Error, Debug)]
/// Error from a websocket client
pub enum WSClientError {
    /// Error during connection setup
    #[error("While connecting {0}")]
    Connection(String),
    /// Passing a broker error
    #[error(transparent)]
    Broker(#[from] BrokerError),
}

#[derive(Error, Debug)]
/// WebSocket server error
pub enum WSSError {
    /// Passing a broker error
    #[error(transparent)]
    Broker(#[from] BrokerError),
    /// Tokio error while joining channels
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    /// Tokio error while getting client
    #[cfg(target_family="unix")]
    #[error(transparent)]
    Client(#[from] tokio_tungstenite::tungstenite::Error),
    /// Generic IO error
    #[error(transparent)]
    IO(#[from] std::io::Error),
}

pub type BrokerWSClient = Broker<WSClientIn, WSClientOut>;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
/// Commands for the websocket client
pub enum WSClientIn {
    /// Send a text message over the websocket connection
    Message(String),
    /// Disconnect the websocket - no further messages will be sent after this message.
    Disconnect,
    /// Connect the websocket - this starts or resets the connection
    Connect,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
/// Websocket client return messages
pub enum WSClientOut {
    /// Text message received over the connection
    Message(String),
    /// The connection has been closed
    Disconnect,
    /// The connection is open now
    Connected,
    /// Something bad happened, and the connection is unstable, but might still work
    Error(String),
}

pub type BrokerWSServer = Broker<WSServerIn, WSServerOut>;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
/// Messages sent by the websocket server
pub enum WSServerOut {
    /// A message has been received from the connection `usize`
    Message(usize, String),
    /// A new connection is available with index `usize`
    NewConnection(usize),
    /// The connection with index `usize` has been closed. There will be no other connection
    /// with the same index unless the websocket server is restarted.
    Disconnection(usize),
    /// The websocket server is now stopped and will not send or receive any more messages.
    Stopped,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
/// Commands for the websocket server
pub enum WSServerIn {
    /// Send a message to the connection with index `usize`. If the connection
    /// doesn't exist, or is closed, nothing happens.
    Message(usize, String),
    /// Request to close the connection with index `usize`. Once this message
    /// has been treated by the websocket server, no further messages will be
    /// sent or received for that connection. If the connection doesn't exist,
    /// or is already closed, nothing happens.
    Close(usize),
    /// Stop the websocket server
    Stop,
}
