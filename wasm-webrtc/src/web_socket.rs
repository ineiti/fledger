use async_trait::async_trait;
use common::signal::websocket::WSError;
use std::{
    cell::RefCell,
    rc::Rc,
};

use log::{info, warn, debug, error};

use wasm_bindgen::prelude::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;

use web_sys::ErrorEvent;
use web_sys::MessageEvent;
use web_sys::WebSocket;

use common::signal::websocket::{MessageCallback, WSMessage, WebSocketConnection};

pub struct WebSocketWasm {
    cb: Rc<RefCell<Option<MessageCallback>>>,
    ws: WebSocket,
    addr: String,
}

impl WebSocketWasm {
    pub fn new(addr: &str) -> Result<WebSocketWasm, WSError> {
        info!("connecting to: {}", addr);
        let ws = WebSocket::new(addr).map_err(|e| WSError::Underlying(format!("{:?}", e)))?;
        let mut wsw = WebSocketWasm {
            cb: Rc::new(RefCell::new(None)),
            ws: ws.clone(),
            addr: addr.to_string(),
        };
        wsw.attach_callbacks();
        Ok(wsw)
    }

    fn attach_callbacks(&mut self) {
        let ws = self.ws.clone();

        // create callback
        let cb_clone = self.cb.clone();
        let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                let s: String = txt.into();
                if let Some(cb) = cb_clone.borrow_mut().as_deref_mut() {
                    cb(WSMessage::MessageString(s));
                }
            } else {
                debug!("message event, received Unknown: {:?}", e);
            }
        }) as Box<dyn FnMut(MessageEvent)>);
        // set message event handler on WebSocket
        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        // forget the callback to keep it alive
        onmessage_callback.forget();

        let cb_clone = self.cb.clone();
        let onerror_callback = Closure::wrap(Box::new(move |e: ErrorEvent| {
            error!("error event: {:?}", e);
            if let Some(cb) = cb_clone.borrow_mut().as_deref_mut() {
                let s: String = e.to_string().into();
                cb(WSMessage::Error(s));
            }
        }) as Box<dyn FnMut(ErrorEvent)>);
        ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
        onerror_callback.forget();

        let cb_clone = self.cb.clone();
        let onopen_callback = Closure::wrap(Box::new(move |_| {
            debug!("socket opened");
            if let Some(cb) = cb_clone.borrow_mut().as_deref_mut() {
                cb(WSMessage::Opened("".to_string()));
            }
        }) as Box<dyn FnMut(JsValue)>);
        ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
        onopen_callback.forget();
    }
}

#[async_trait(?Send)]
impl WebSocketConnection for WebSocketWasm {
    fn send(&mut self, msg: String) -> Result<(), WSError> {
        if self.ws.ready_state() != WebSocket::OPEN {
            warn!("Websocket is not open - trying to reconnect");
            self.reconnect()?;
            Err(WSError::Underlying("Send while not connected".into()))
        } else {
                self.ws
                    .send_with_str(&msg)
                    .map_err(|e| WSError::Underlying(format!("Error while sending: {:?}", e)))?;
            Ok(())
        }
    }

    fn set_cb_wsmessage(&mut self, cb: MessageCallback) {
        self.cb.borrow_mut().replace(cb);
    }

    fn reconnect(&mut self) -> Result<(), WSError> {
        warn!("Reconnecting websocket");
        if let Err(e) = self.ws.close() {
            error!("Error while closing: {:?}", e);
        }
        debug!("Re-opening websocket");
        self.ws = WebSocket::new(&self.addr).map_err(|e| WSError::Underlying(format!("{:?}", e)))?;
        self.attach_callbacks();
        Ok(())
    }
}
