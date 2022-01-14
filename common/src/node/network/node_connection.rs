use crate::{signal::web_rtc::PeerInfo};
use types::nodeids::U256;
use std::sync::{Arc, Mutex};
use thiserror::Error;

use crate::{
    broker::Broker,
    node::network::connection_state::{CSEnum, CSError, ConnectionState},
    signal::web_rtc::WebRTCSpawner,
};

#[derive(Error, Debug)]
pub enum NCError {
    #[error(transparent)]
    ConnectionState(#[from] CSError),
    #[error("Couldn't use output queue")]
    OutputQueue,
}

/// There might be up to two connections per remote node.
/// This is in the case both nodes try to set up a connection at the same time.
/// This race condition is very difficult to catch, so it's easier to just allow
/// two connections per remote node.
/// If a second, third, or later incoming connection from the same node happens, the previous
/// connection is considered stale and discarded.
pub struct NodeConnection {
    // outgoing connections are the preferred ones.
    pub outgoing: ConnectionState,
    // incoming connections are connections initiated from another node.
    pub incoming: ConnectionState,

    id_remote: U256,

    msg_queue: Vec<String>,
}

impl NodeConnection {
    pub async fn new(
        web_rtc: Arc<Mutex<WebRTCSpawner>>,
        broker: Broker,
        id_local: U256,
        id_remote: U256,
    ) -> Result<NodeConnection, NCError> {
        let nc = NodeConnection {
            outgoing: ConnectionState::new(
                false,
                Arc::clone(&web_rtc),
                broker.clone(),
                id_local,
                id_remote,
            )
            .await?,
            incoming: ConnectionState::new(
                true,
                Arc::clone(&web_rtc),
                broker.clone(),
                id_local,
                id_remote,
            )
            .await?,
            id_remote,
            msg_queue: vec![],
        };
        Ok(nc)
    }

    /// Tries to send a message over the webrtc connection.
    /// If the connection is in setup phase, the message is queued.
    /// If the connection is idle, an error is returned.
    pub async fn send(&mut self, msg: String) -> Result<(), NCError> {
        let conn_result = {
            if self.outgoing.get_connection_open().await? {
                Some(&mut self.outgoing)
            } else {
                if self.incoming.get_connection_open().await? {
                    Some(&mut self.incoming)
                } else {
                    None
                }
            }
        };
        match conn_result {
            Some(conn) => {
                // Correctly orders the message after already waiting messages and
                // avoids an if to check if the queue is full...
                self.msg_queue.push(msg);
                for m in self.msg_queue.splice(.., vec![]).collect::<Vec<String>>() {
                    conn.send(m)
                        .await
                        .map_err(|_| NCError::ConnectionState(CSError::InputQueue))?;
                }
                Ok(())
            }
            None => {
                if self.outgoing.get_connection_state() == CSEnum::Idle {
                    self.msg_queue.push(msg);
                    self.outgoing.start_connection(None).await?;
                }
                Ok(())
            }
        }
    }

    pub async fn process_ws(&mut self, pi: PeerInfo) -> Result<(), NCError> {
        match self.id_remote == pi.id_init {
            false => &mut self.outgoing,
            true => &mut self.incoming,
        }
        .process_peer_message(pi.message)
        .await?;
        Ok(())
    }
}
