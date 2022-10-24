use async_trait::async_trait;
use futures::lock::Mutex;
use std::sync::Arc;
use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
use web_sys::{ErrorEvent, MessageEvent, WebSocket};

use flmodules::broker::{Broker, Subsystem, SubsystemHandler};

use crate::websocket::{WSClientError, WSClientInput, WSClientMessage, WSClientOutput};

pub struct WebSocketClient {
    ws: Arc<Mutex<WebSocket>>,
}

unsafe impl Send for WebSocketClient {}

impl WebSocketClient {
    pub async fn connect(url: &str) -> Result<Broker<WSClientMessage>, WSClientError> {
        log::info!("connecting to: {}", url);
        let ws = WebSocket::new(url).map_err(|e| WSClientError::Connection(format!("{:?}", e)))?;
        let mut wsw = WebSocketClient {
            ws: Arc::new(Mutex::new(ws)),
        };
        let mut broker = wsw.attach_callbacks().await;
        broker
            .add_subsystem(Subsystem::Handler(Box::new(wsw)))
            .await?;
        Ok(broker)
    }

    async fn attach_callbacks(&mut self) -> Broker<WSClientMessage> {
        let broker = Broker::new();
        let ws = self.ws.lock().await;

        // create callback
        let mut broker_clone = broker.clone();
        let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                let txt_str = txt.as_string().unwrap();
                broker_clone
                    .emit_msg(WSClientOutput::Message(txt_str).into())
                    .err()
                    .map(|e| log::error!("On_message_callback error: {e:?}"));
            } else {
                log::warn!("message event, received Unknown: {:?}", e);
            }
        }) as Box<dyn FnMut(MessageEvent)>);
        // set message event handler on WebSocket
        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        // forget the callback to keep it alive
        onmessage_callback.forget();

        let mut broker_clone = broker.clone();
        let onerror_callback = Closure::wrap(Box::new(move |e: ErrorEvent| {
            log::error!("error event: {:?}", e);
            broker_clone
                .emit_msg(WSClientMessage::Output(WSClientOutput::Error(
                    e.as_string().unwrap_or("not an error string".into()),
                )))
                .err()
                .map(|e| log::error!("On_error_callback error: {e:?}"));
        }) as Box<dyn FnMut(ErrorEvent)>);
        ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
        onerror_callback.forget();

        let mut broker_clone = broker.clone();
        let onopen_callback = Closure::wrap(Box::new(move |_| {
            broker_clone
                .emit_msg(WSClientMessage::Output(WSClientOutput::Connected))
                .err()
                .map(|e| log::error!("On_open_callback error: {e:?}"));
        }) as Box<dyn FnMut(JsValue)>);
        ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
        onopen_callback.forget();

        broker
    }
}

#[async_trait(?Send)]
impl SubsystemHandler<WSClientMessage> for WebSocketClient {
    async fn messages(&mut self, msgs: Vec<WSClientMessage>) -> Vec<WSClientMessage> {
        if let Some(ws) = self.ws.try_lock() {
            for msg in msgs {
                if let WSClientMessage::Input(msg_in) = msg {
                    match msg_in {
                        WSClientInput::Message(msg) => {
                            if ws.ready_state() != WebSocket::OPEN {
                                log::error!("WebSocket is not open for {msg:?}");
                                return vec![];
                            }
                            ws.send_with_str(&msg)
                                .err()
                                .map(|e| log::error!("Error sending message: {:?}", e));
                        }
                        WSClientInput::Disconnect => {
                            // ws.disconnect();
                        }
                    }
                }
            }
        }
        vec![]
    }
}
