/// Very simple rendez-vous server that allows new nodes to send their node_info.
/// It also allows the nodes to fetch all existing node_infos of all the other nodes.
///
/// TODO: use the `newID` endpoint to authentify the nodes' id
// mod node_list;
mod state;

use async_trait::async_trait;

use std::{
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use tungstenite::{accept, protocol::Role, Message, WebSocket};

use common::{
    node::ext_interface::Logger,
    signal::websocket::{
        MessageCallbackSend, NewConnectionCallback, WSMessage, WebSocketConnectionSend,
        WebSocketServer,
    },
};

use state::ServerState;

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
        let server = TcpListener::bind("0.0.0.0:8765").unwrap();
        let uws = UnixWebSocket {
            cb: Arc::new(Mutex::new(None)),
        };
        let uws_cl = Arc::clone(&uws.cb);
        thread::spawn(move || {
            for stream in server.incoming() {
                let mut cb_mutex = uws_cl.lock().unwrap();
                if let Some(cb) = cb_mutex.as_mut() {
                    match UnixWSConnection::new(stream.unwrap()){
                        Ok(conn) => cb(conn),
                        Err(e) => println!("Error while getting connection: {:?}", e),
                    }
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
    fn new(stream: TcpStream) -> Result<Box<UnixWSConnection>, String> {
        let websocket = accept(stream).map_err(|e| e.to_string())?;
        let mut uwsc = Box::new(UnixWSConnection {
            websocket,
            cb: Arc::new(Mutex::new(None)),
        });
        let cb_clone = Arc::clone(&uwsc.cb);

        let ts_clone = uwsc.websocket.get_mut().try_clone().unwrap();
        let mut ws_clone = WebSocket::from_raw_socket(ts_clone, Role::Server, None);
        thread::spawn(move || loop {
            match ws_clone.read_message() {
                Ok(msg) => {
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
        Ok(uwsc)
    }
}

#[async_trait]
impl WebSocketConnectionSend for UnixWSConnection {
    fn set_cb_wsmessage(&mut self, cb: MessageCallbackSend) {
        let mut cb_lock = self.cb.lock().unwrap();
        cb_lock.replace(cb);
    }

    async fn send(&mut self, msg: String) -> Result<(), String> {
        self.websocket.write_message(Message::Text(msg)).map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn main() {
    let logger = Box::new(StdOutLogger {});
    let ws = Box::new(UnixWebSocket::new());
    let state = ServerState::new(logger, ws);
    println!("Server started and listening on port 8765");
    state.wait_done(Duration::from_secs(30));
}
