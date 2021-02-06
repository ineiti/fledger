use async_trait::async_trait;
use std::{cell::RefCell, rc::Rc};

use common::websocket::MessageCallback;
use common::websocket::WSMessage;
use common::websocket::WebSocketConnection;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;

use web_sys::ErrorEvent;
use web_sys::MessageEvent;
use web_sys::WebSocket;

pub struct WebSocketWasm {
    cb: Rc<RefCell<Option<MessageCallback>>>,
    ws: WebSocket,
}

impl WebSocketWasm {
    pub fn new(addr: &str) -> Result<WebSocketWasm, JsValue> {
        console_log!("connecting to: {}", addr);
        let ws = WebSocket::new(addr)?;
        let wsw = WebSocketWasm {
            cb: Rc::new(RefCell::new(None)),
            ws: ws.clone(),
        };

        // create callback
        console_log!("creating onmessage callback");
        let cb_clone = wsw.cb.clone();
        let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                let s: String = txt.into();
                if let Some(cb) = cb_clone.borrow_mut().as_deref_mut() {
                    cb(WSMessage::MessageString(s));
                }
            } else {
                console_log!("message event, received Unknown: {:?}", e);
            }
        }) as Box<dyn FnMut(MessageEvent)>);
        // set message event handler on WebSocket
        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        // forget the callback to keep it alive
        onmessage_callback.forget();

        console_log!("creating onerror callback");
        let cb_clone = wsw.cb.clone();
        let onerror_callback = Closure::wrap(Box::new(move |e: ErrorEvent| {
            console_log!("error event: {:?}", e);
            if let Some(cb) = cb_clone.borrow_mut().as_deref_mut() {
                let s: String = e.to_string().into();
                cb(WSMessage::Error(s));
            }
        }) as Box<dyn FnMut(ErrorEvent)>);
        ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
        onerror_callback.forget();

        console_log!("creating onopen callback");
        let cb_clone = wsw.cb.clone();
        let onopen_callback = Closure::wrap(Box::new(move |_| {
            console_log!("socket opened");
            if let Some(cb) = cb_clone.borrow_mut().as_deref_mut() {
                cb(WSMessage::Opened("".to_string()));
            }
        }) as Box<dyn FnMut(JsValue)>);
        ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
        onopen_callback.forget();

        console_log!("websocket done");
        Ok(wsw)
    }
}

#[async_trait(?Send)]
impl WebSocketConnection for WebSocketWasm {
    fn send(&mut self, msg: String) -> Result<(), String> {
        let _ = self
            .ws
            .send_with_str(&msg);
        Ok(())
    }

    fn set_cb_wsmessage(&mut self, cb: MessageCallback) {
        self.cb.borrow_mut().replace(cb);
    }
}
