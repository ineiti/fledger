use std::sync::{
    mpsc::{self, Receiver, Sender},
    Arc, Mutex,
};

use async_trait::async_trait;

use super::{
    web_rtc::{WSSignalMessageFromNode, WSSignalMessageToNode},
    websocket::{
        MessageCallback, NewConnectionCallback, WSError, WSMessage, WebSocketConnection,
        WebSocketServer,
    },
};

pub struct WebSocketSimul {
    int: Arc<Mutex<Intern>>,
}

impl WebSocketSimul {
    pub fn new() -> Self {
        Self {
            int: Arc::new(Mutex::new(Intern::new())),
        }
    }

    pub fn new_server(&mut self) -> WebSocketServerDummy {
        let int = self.int.lock().unwrap();
        if int.server_cc.is_some() {
            panic!("WebSocketSimul::new_server: already called");
        }
        WebSocketServerDummy::new(Arc::clone(&self.int))
    }

    pub fn new_connection(&mut self) -> (usize, WebSocketConnectionDummy) {
        let cb = Arc::new(Mutex::new(None));
        let (tx, rx) = mpsc::channel();
        let conn = WebSocketConnectionDummy::new(Arc::clone(&cb), tx);
        (self.int.lock().unwrap().new_connection(cb, rx), conn)
    }

    pub fn new_incoming_connection(&mut self) -> usize {
        let (index, conn) = self.new_connection();
        if let Some(cc) = self.int.lock().unwrap().server_cc.as_mut() {
            cc(Box::new(conn));
        }
        index
    }

    pub fn send_msg(&mut self, conn: usize, msg: String) {
        let int = self.int.lock().unwrap();
        if let Some(cb) = int.conn_cb[conn].lock().unwrap().as_mut() {
            cb(WSMessage::MessageString(msg));
        };
    }

    pub fn send_ws_msg_from(
        &mut self,
        conn: usize,
        msg: WSSignalMessageFromNode,
    ) -> Result<(), WSError> {
        self.send_msg(
            conn,
            serde_json::to_string(&msg).map_err(|e| WSError::Underlying(e.to_string()))?,
        );
        Ok(())
    }

    pub fn send_ws_msg_to(
        &mut self,
        conn: usize,
        msg: WSSignalMessageToNode,
    ) -> Result<(), WSError> {
        self.send_msg(
            conn,
            serde_json::to_string(&msg).map_err(|e| WSError::Underlying(e.to_string()))?,
        );
        Ok(())
    }

    pub fn recv_msg(&mut self, conn: usize) -> Vec<String> {
        let int = self.int.lock().unwrap();
        int.conn_tx[conn].try_iter().collect()
    }

    pub fn recv_ws_msg_from(
        &mut self,
        conn: usize,
    ) -> Result<Vec<WSSignalMessageFromNode>, WSError> {
        self.recv_msg(conn)
            .iter()
            .map(|s| serde_json::from_str(s).map_err(|e| WSError::Underlying(e.to_string())))
            .collect()
    }

    pub fn recv_ws_msg_to(&mut self, conn: usize) -> Result<Vec<WSSignalMessageToNode>, WSError> {
        self.recv_msg(conn)
            .iter()
            .map(|s| serde_json::from_str(s).map_err(|e| WSError::Underlying(e.to_string())))
            .collect()
    }

    pub fn process(&mut self) {
        self.int.lock().unwrap().process()
    }
}

pub struct Intern {
    conn_cb: Vec<Arc<Mutex<Option<MessageCallback>>>>,
    conn_tx: Vec<Receiver<String>>,
    conn_link: Vec<(usize, usize)>,
    server_cc: Option<NewConnectionCallback>,
}

impl Intern {
    pub fn new() -> Self {
        Self {
            conn_cb: vec![],
            conn_tx: vec![],
            conn_link: vec![],
            server_cc: None,
        }
    }

    pub fn new_connection(
        &mut self,
        cb: Arc<Mutex<Option<MessageCallback>>>,
        rx: Receiver<String>,
    ) -> usize {
        self.conn_cb.push(cb);
        self.conn_tx.push(rx);
        self.conn_cb.len() - 1
    }

    pub fn new_link(&mut self, from: usize, to: usize) {
        self.conn_link.push((from, to));
    }

    pub fn process(&mut self) {
        for (from, to) in self.conn_link.iter() {
            for msg in self.conn_tx[*from].try_iter() {
                let mut conn_cb = self.conn_cb[*to].lock().unwrap();
                if let Some(cb) = conn_cb.as_mut() {
                    cb(WSMessage::MessageString(msg));
                }
            }
        }
    }
}

pub struct WebSocketServerDummy {
    int: Arc<Mutex<Intern>>,
}

impl WebSocketServer for WebSocketServerDummy {
    fn set_cb_connection(&mut self, cb: NewConnectionCallback) {
        let mut int = self.int.lock().unwrap();
        int.server_cc = Some(cb);
    }
}

impl WebSocketServerDummy {
    pub fn new(int: Arc<Mutex<Intern>>) -> Self {
        Self { int }
    }
}

pub struct WebSocketConnectionDummy {
    cb: Arc<Mutex<Option<MessageCallback>>>,
    tx: Sender<String>,
}

impl WebSocketConnectionDummy {
    pub fn new(
        cb: Arc<Mutex<Option<MessageCallback>>>,
        tx: Sender<String>,
    ) -> WebSocketConnectionDummy {
        WebSocketConnectionDummy { cb, tx }
    }
}

#[async_trait]
impl WebSocketConnection for WebSocketConnectionDummy {
    fn set_cb_wsmessage(&mut self, cb: MessageCallback) {
        if let Ok(mut cb_lock) = self.cb.try_lock() {
            *cb_lock = Some(cb);
        }
    }

    fn send(&mut self, msg: String) -> Result<(), WSError> {
        self.tx
            .send(msg)
            .map_err(|e| WSError::Underlying(e.to_string()))
    }

    fn reconnect(&mut self) -> Result<(), WSError> {
        Err(WSError::Underlying("not implemented".into()))
    }
}
