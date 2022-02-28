use async_trait::async_trait;

use std::{
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
};
use tungstenite::{accept, protocol::Role, Message, WebSocket};

use flnet::signal::websocket::{
    MessageCallbackSend, NewConnectionCallback, WSError, WSMessage, WebSocketConnection,
    WebSocketServer,
};

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
    pub fn new() -> UnixWebSocket {
        let server = TcpListener::bind("0.0.0.0:8765").unwrap();
        let uws = UnixWebSocket {
            cb: Arc::new(Mutex::new(None)),
        };
        let uws_cl = Arc::clone(&uws.cb);
        thread::spawn(move || {
            for stream in server.incoming() {
                let mut cb_mutex = uws_cl.lock().unwrap();
                if let Some(cb) = cb_mutex.as_mut() {
                    match UnixWSConnection::new(stream.unwrap()) {
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

impl UnixWSConnection {
    fn new(stream: TcpStream) -> Result<Box<UnixWSConnection>, WSError> {
        let websocket = accept(stream).map_err(|e| WSError::Underlying(e.to_string()))?;
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
                    log::warn!("Closing connection: {:?}", e);
                    return;
                }
            }
        });
        Ok(uwsc)
    }
}

#[async_trait]
impl WebSocketConnection for UnixWSConnection {
    fn set_cb_wsmessage(&mut self, cb: MessageCallbackSend) {
        let mut cb_lock = self.cb.lock().unwrap();
        cb_lock.replace(cb);
    }

    async fn send(&mut self, msg: String) -> Result<(), WSError> {
        self.websocket
            .write_message(Message::Text(msg))
            .map_err(|e| WSError::Underlying(e.to_string()))?;
        Ok(())
    }
}
