use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WSError {
    #[error("In underlying system: {0}")]
    Underlying(String),
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum WSMessage {
    MessageString(String),
    Error(String),
    Closed(String),
    Opened(String),
}

#[cfg(not(target_arch = "wasm32"))]
mod websocket_server {
    use super::*;

    pub type MessageCallback = Box<dyn FnMut(WSMessage) + Send>;

    #[async_trait]
    pub trait WebSocketConnection: Send {
        fn set_cb_wsmessage(&mut self, cb: MessageCallback);
        fn send(&mut self, msg: String) -> Result<(), WSError>;
        fn reconnect(&mut self) -> Result<(), WSError>;
    }
    
    pub type NewConnectionCallback = Box<dyn FnMut(Box<dyn WebSocketConnection + Send>) + Send>;
    
    pub trait WebSocketServer {
        fn set_cb_connection(&mut self, cb: NewConnectionCallback);
    }    
}

#[cfg(target_arch = "wasm32")]
mod websocket_server {
    use super::*;
    pub type MessageCallback = Box<dyn FnMut(WSMessage)>;

    #[async_trait(?Send)]
    pub trait WebSocketConnection {
        fn set_cb_wsmessage(&mut self, cb: MessageCallback);
        fn send(&mut self, msg: String) -> Result<(), WSError>;
        fn reconnect(&mut self) -> Result<(), WSError>;
    }

    pub type NewConnectionCallback = Box<dyn FnMut(Box<dyn WebSocketConnection>)>;
    
    pub trait WebSocketServer {
        fn set_cb_connection(&mut self, cb: NewConnectionCallback);
    }    
}

pub use websocket_server::*;