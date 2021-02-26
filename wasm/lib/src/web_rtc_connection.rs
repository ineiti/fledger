use wasm_bindgen::{prelude::Closure, JsCast};
use web_sys::{MessageEvent, RtcDataChannel};

use common::signal::web_rtc::{WebRTCConnection, WebRTCMessageCB};

pub struct WebRTCConnectionWasm {
    dc: RtcDataChannel,
}

impl WebRTCConnectionWasm {
    pub fn new(dc: RtcDataChannel) -> Box<dyn WebRTCConnection> {
        Box::new(WebRTCConnectionWasm { dc })
    }
}

impl WebRTCConnection for WebRTCConnectionWasm {
    /// Send a message to the other node. This call blocks until the message
    /// is queued.
    fn send(&self, s: String) -> Result<(), String> {
        self.dc
            .send_with_str(&s)
            .map_err(|e| format!("{:?}", e))
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
}
