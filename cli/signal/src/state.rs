use log::{error, debug};
use std::sync::mpsc::channel;
use std::sync::Mutex;
use std::thread;
use std::{sync::Arc, time::Duration};

use common::{
    types::U256,
    signal::websocket::{WSMessage, WebSocketConnectionSend, WebSocketServer},
};

mod internal;
mod node_entry;
use internal::Internal;
use node_entry::NodeEntry;
use crate::config::Config;

pub struct ServerState {
    int: Arc<Mutex<Internal>>,
    config: Config,
}

/// This holds the logic of the signalling server.
/// It can do the following;
/// - listen for incoming websocket requests
/// - handle webrtc signalling setup
impl ServerState {
    pub fn new(config: Config, mut ws: Box<dyn WebSocketServer>) -> Result<ServerState, String> {
        let ss = ServerState {
            int: Internal::new(config.clone())?,
            config,
        };
        let int_cl = Arc::clone(&ss.int);
        ws.set_cb_connection(Box::new(move |conn| {
            ServerState::cb_connection(Arc::clone(&int_cl), conn)
        }));
        Ok(ss)
    }

    /// Treats new connections from websockets.
    fn cb_connection(int: Arc<Mutex<Internal>>, mut conn: Box<dyn WebSocketConnectionSend + Send>) {
        let challenge = U256::rnd();
        let ch_cl = challenge.clone();
        let int_clone = Arc::clone(&int);
        let (tx, rx) = channel::<WSMessage>();
        conn.set_cb_wsmessage(Box::new(move |cb| {
            if let Err(e) = tx.send(cb){
                error!("Couldn't send over channel: {}", e);
                return;
            }
            if let Ok(mut ic) = int_clone.try_lock() {
                let msgs: Vec<WSMessage> = rx.try_iter().collect();
                for msg in msgs {
                    ic.cb_msg(&ch_cl, msg);
                }
            } else {
                debug!("Couldn't lock int_clone - message waits in the channel");
            }
        }));

        if let Ok(mut int_lock) = int.try_lock() {
            int_lock
                .nodes
                .insert(challenge.clone(), NodeEntry::new(challenge, conn));
        }
    }

    /// Waits for everything done while calling cleanup from time to time.
    pub fn wait_done(&self) {
        loop {
            thread::sleep(Duration::from_millis(self.config.cleanup_interval * 1000));
            self.int.lock().unwrap().cleanup();
        }
    }
}

#[cfg(test)]
mod tests {}
