use async_trait::async_trait;
use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
use web_sys::{ErrorEvent, MessageEvent, WebSocket};

use crate::broker::{Broker, SubsystemHandler};

use crate::tasks::{now, wait_ms};
use crate::web_rtc::websocket::{WSClientError, WSClientIn, WSClientOut};

pub struct WebSocketClient {
    url: String,
    ws: Option<WebSocket>,
    broker: Broker<WSClientIn, WSClientOut>,
    last_connection: i64,
}

impl WebSocketClient {
    pub async fn connect(
        url: &str,
    ) -> anyhow::Result<Broker<WSClientIn, WSClientOut>> {
        let wsw = WebSocketClient {
            url: url.to_string(),
            ws: None,
            broker: Broker::new(),
            last_connection: 0,
        };
        let mut broker = wsw.broker.clone();
        broker.add_handler(Box::new(wsw)).await?;
        broker.emit_msg_in(WSClientIn::Connect)?;
        Ok(broker)
    }

    async fn connect_ws(&mut self) -> anyhow::Result<()> {
        if self.ws.is_some() && now() / 1000 < self.last_connection + 10 {
            return Ok(());
        }
        self.last_connection = now() / 1000;

        if let Some(ws) = self.ws.take() {
            log::debug!("Reconnecting websocket to {}", self.url);
            ws.set_onmessage(None);
            ws.set_onerror(None);
            ws.set_onopen(None);
            ws.close()
                .map_err(|e| WSClientError::Connection(format!("{:?}", e)))?;
        } else {
            log::debug!("Connecting websocket to {}", self.url);
        }
        self.ws = Some(
            WebSocket::new(&self.url).map_err(|e| WSClientError::Connection(format!("{:?}", e)))?,
        );
        self.attach_callbacks().await;

        // Wait up to 5 seconds for connection, so that another connection request doesn't
        // interrupt the ongoing connection.
        for _ in 0..50 {
            if self.ws.as_ref().unwrap().ready_state() == WebSocket::OPEN {
                log::info!("Successfully opened WebSocket connection to {}", self.url);
                return Ok(());
            }
            wait_ms(100).await;
        }

        Ok(())
    }

    async fn attach_callbacks(&mut self) {
        if let Some(ws) = self.ws.as_ref() {
            // create callback
            let mut broker_clone = self.broker.clone();
            let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
                if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                    let txt_str = txt.as_string().unwrap();
                    broker_clone
                        .emit_msg_out(WSClientOut::Message(txt_str))
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

            let mut broker_clone = self.broker.clone();
            let onerror_callback = Closure::wrap(Box::new(move |_: ErrorEvent| {
                broker_clone
                    .emit_msg_out(WSClientOut::Error("WS-error".into()))
                    .err()
                    .map(|e| log::error!("On_error_callback error: {e:?}"));
            }) as Box<dyn FnMut(ErrorEvent)>);
            ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
            onerror_callback.forget();

            let mut broker_clone = self.broker.clone();
            let onopen_callback = Closure::wrap(Box::new(move |_| {
                broker_clone
                    .emit_msg_out(WSClientOut::Connected)
                    .err()
                    .map(|e| log::error!("On_open_callback error: {e:?}"));
            }) as Box<dyn FnMut(JsValue)>);
            ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
            onopen_callback.forget();
        }
    }
}

#[async_trait(?Send)]
impl SubsystemHandler<WSClientIn, WSClientOut> for WebSocketClient {
    async fn messages(&mut self, msgs: Vec<WSClientIn>) -> Vec<WSClientOut> {
        for msg in msgs {
            match msg {
                WSClientIn::Message(msg) => {
                    if let Some(ws) = self.ws.as_mut() {
                        if ws.ready_state() != WebSocket::OPEN {
                            log::debug!("WebSocket is not open for {msg:?}");
                            if let Err(e) = self.connect_ws().await {
                                log::warn!("While reconnecting: {e}");
                            }
                            return vec![];
                        }
                        ws.send_with_str(&msg)
                            .err()
                            .map(|e| log::error!("Error sending message: {:?}", e));
                    }
                }
                WSClientIn::Disconnect => {
                    // ws.disconnect();
                }
                WSClientIn::Connect => {
                    if let Err(e) = self.connect_ws().await {
                        log::warn!("While reconnecting: {e}");
                    }
                }
            }
        }
        vec![]
    }
}
