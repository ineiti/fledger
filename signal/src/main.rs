/// Very simple rendez-vous server that allows new nodes to send their node_info.
/// It also allows the nodes to fetch all existing node_infos of all the other nodes.
///
/// TODO: use the `newID` endpoint to authentify the nodes' public key
// mod node_list;
mod state;

use async_trait::async_trait;

use std::{
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
};
use tungstenite::{accept, protocol::Role, Message, WebSocket};

use common::websocket::{
    NewConnectionCallback, WSMessage, WebSocketConnectionSend, WebSocketServer,
};
use common::{ext_interface::Logger, websocket::MessageCallbackSend};
use state::ServerState;

fn main() {
    let logger = Box::new(StdOutLogger {});
    let ws = Box::new(UnixWebSocket::new());
    let state = ServerState::new(logger, ws);
    state.wait_done();
}

pub struct StdOutLogger {}

impl Logger for StdOutLogger {
    fn info(&self, s: &str) {
        println!("{}", s);
    }

    fn warn(&self, s: &str) {
        println!("{}", s);
    }

    fn error(&self, s: &str) {
        println!("{}", s);
    }

    fn clone(&self) -> Box<dyn Logger> {
        Box::new(StdOutLogger {})
    }
}

pub struct UnixWebSocket {
    cb: Arc<Mutex<Option<NewConnectionCallback>>>,
}

impl WebSocketServer for UnixWebSocket {
    fn set_cb_connection(&mut self, cb: NewConnectionCallback) {
        let scb = Arc::clone(&self.cb);
        scb.lock().unwrap().replace(Box::new(cb));
    }
}

impl UnixWebSocket {
    fn new() -> UnixWebSocket {
        let server = TcpListener::bind("127.0.0.1:8765").unwrap();
        let uws = UnixWebSocket {
            cb: Arc::new(Mutex::new(None)),
        };
        let uws_cl = Arc::clone(&uws.cb);
        thread::spawn(move || {
            for stream in server.incoming() {
                let mut cb_mutex = uws_cl.lock().unwrap();
                if let Some(cb) = cb_mutex.as_mut() {
                    cb(UnixWSConnection::new(stream.unwrap()));
                }
            }
        });
        uws
    }
}

struct UnixWSConnection {
    websocket: WebSocket<TcpStream>,
    cb: Arc<Mutex<Option<MessageCallbackSend>>>,
}

unsafe impl Send for UnixWSConnection {}

unsafe impl Sync for UnixWSConnection {}

impl UnixWSConnection {
    fn new(stream: TcpStream) -> Box<UnixWSConnection> {
        let mut uwsc = Box::new(UnixWSConnection {
            websocket: accept(stream).unwrap(),
            cb: Arc::new(Mutex::new(None)),
        });
        let cb_clone = Arc::clone(&uwsc.cb);

        let ts_clone = uwsc.websocket.get_mut().try_clone().unwrap();
        let mut ws_clone = WebSocket::from_raw_socket(ts_clone, Role::Server, None);
        thread::spawn(move || loop {
            match ws_clone.read_message() {
                Ok(msg) => {
                    println!("Got a raw message: {:?}", msg);
                    if msg.is_text() {
                        let mut cb_mutex = cb_clone.lock().unwrap();
                        if let Some(cb) = cb_mutex.as_mut() {
                            cb(WSMessage::MessageString(msg.to_text().unwrap().to_string()));
                        }
                    }
                }
                Err(e) => {
                    println!("Closing connection: {:?}", e);
                    return;
                }
            }
        });
        uwsc
    }
}

#[async_trait]
impl WebSocketConnectionSend for UnixWSConnection {
    fn set_cb_wsmessage(&mut self, cb: MessageCallbackSend) {
        let mut cb_lock = self.cb.lock().unwrap();
        cb_lock.replace(cb);
    }

    async fn send(&mut self, msg: String) -> Result<(), String> {
        println!("sending: {:?}", msg);
        self.websocket.write_message(Message::Text(msg)).unwrap();
        Ok(())
    }
}
