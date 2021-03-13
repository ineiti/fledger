use std::sync::Mutex;
use std::thread;
use std::{sync::Arc, time::Duration};

use common::{
    node::{ext_interface::Logger, types::U256},
    signal::websocket::{WebSocketConnectionSend, WebSocketServer},
};

mod internal;
mod node_entry;
use internal::Internal;
use node_entry::NodeEntry;

pub struct ServerState {
    int: Arc<Mutex<Internal>>,
}

/// This holds the logic of the signalling server.
/// It can do the following;
/// - listen for incoming websocket requests
/// - handle webrtc signalling setup
impl ServerState {
    pub fn new(logger: Box<dyn Logger>, mut ws: Box<dyn WebSocketServer>) -> ServerState {
        let ss = ServerState {
            int: Internal::new(logger),
        };
        let int_cl = Arc::clone(&ss.int);
        ws.set_cb_connection(Box::new(move |conn| {
            ServerState::cb_connection(Arc::clone(&int_cl), conn)
        }));
        ss
    }

    /// Treats new connections from websockets.
    fn cb_connection(int: Arc<Mutex<Internal>>, mut conn: Box<dyn WebSocketConnectionSend + Send>) {
        let challenge = U256::rnd();
        let ch_cl = challenge.clone();
        let int_clone = Arc::clone(&int);
        conn.set_cb_wsmessage(Box::new(move |cb| {
            if let Ok(mut ic) = int_clone.try_lock() {
                ic.cb_msg(&ch_cl, cb)
            }
        }));

        if let Ok(mut int_lock) = int.try_lock() {
            let logger = int_lock.logger.clone();
            int_lock
                .nodes
                .insert(challenge.clone(), NodeEntry::new(logger, challenge, conn));
        }
    }

    /// Waits for everything done while calling cleanup from time to time.
    pub fn wait_done(&self, delay: Duration) {
        loop {
            thread::sleep(delay);
            self.int.lock().unwrap().cleanup(delay);
        }
    }

    // pub fn send_message(&self, entry: &U256, msg: Message) {
    //     let int_arc = Arc::clone(&self.int);
    //     let mut int = int_arc.lock().unwrap();
    //     int.send_message(entry, msg);
    // }
}

#[cfg(test)]
mod tests {}
