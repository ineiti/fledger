use async_trait::async_trait;

use wasm_bindgen::{prelude::Closure, JsCast};
use web_sys::{console::log_1, MessageEvent, RtcDataChannel, RtcPeerConnection};

use common::signal::web_rtc::{ConnectionStateMap, WebRTCConnection, WebRTCMessageCB};

pub struct WebRTCConnectionWasm {
    dc: RtcDataChannel,
    conn: RtcPeerConnection,
}

impl WebRTCConnectionWasm {
    pub fn new(dc: RtcDataChannel, conn: RtcPeerConnection) -> Box<dyn WebRTCConnection> {
        Box::new(WebRTCConnectionWasm { dc, conn })
    }
}

#[async_trait(?Send)]
impl WebRTCConnection for WebRTCConnectionWasm {
    /// Send a message to the other node. This call blocks until the message
    /// is queued.
    fn send(&self, s: String) -> Result<(), String> {
        self.dc.send_with_str(&s).map_err(|e| format!("{:?}", e))
    }

    /// Sets the callback for incoming messages.
    fn set_cb_message(&self, mut cb: WebRTCMessageCB) {
        let onmessage_callback =
            Closure::wrap(
                Box::new(move |ev: MessageEvent| match ev.data().as_string() {
                    Some(message) => {
                        cb(message);
                    }
                    None => {}
                }) as Box<dyn FnMut(MessageEvent)>,
            );
        self.dc
            .set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        onmessage_callback.forget();
    }

    async fn get_state(&self) -> Result<ConnectionStateMap, String> {
        let conn_stats: js_sys::Map = wasm_bindgen_futures::JsFuture::from(self.conn.get_stats())
            .await
            .unwrap()
            .into();
        conn_stats.for_each(&mut |v, k| log_1(&format!("- {:?}: {:?}", k, v).into()));
        Ok(ConnectionStateMap {
            delay_ms: 0,
            tx_bytes: 0,
            rx_bytes: 0,
            stun_remote: false,
            stun_local: false,
        })
    }
}
