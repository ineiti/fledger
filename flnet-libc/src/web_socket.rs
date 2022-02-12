use flnet::signal::websocket::{MessageCallback, WSError, WebSocketConnection};

pub struct WebSocketLibc {}

impl WebSocketConnection for WebSocketLibc {
    fn set_cb_wsmessage(&mut self, _cb: MessageCallback) {
        todo!()
    }

    fn send(&mut self, _msg: String) -> Result<(), WSError> {
        todo!()
    }

    fn reconnect(&mut self) -> Result<(), WSError> {
        todo!()
    }
}
