use async_trait::async_trait;

use wasm_bindgen::{prelude::Closure, JsCast};
use web_sys::{MessageEvent, RtcDataChannel, RtcPeerConnection, RtcSignalingState};

use flnet::signal::web_rtc::{
    ConnType, ConnectionError, ConnectionStateMap, SignalingState, WebRTCConnection,
    WebRTCMessageCB,
};

pub struct WebRTCConnectionWasm {
    dc: RtcDataChannel,
    conn: RtcPeerConnection,
}

impl WebRTCConnectionWasm {
    pub fn new_box(dc: RtcDataChannel, conn: RtcPeerConnection) -> Box<dyn WebRTCConnection> {
        Box::new(WebRTCConnectionWasm { dc, conn })
    }
}

#[async_trait(?Send)]
impl WebRTCConnection for WebRTCConnectionWasm {
    /// Send a message to the other node. This call blocks until the message
    /// is queued.
    fn send(&self, s: String) -> Result<(), ConnectionError> {
        self.dc
            .send_with_str(&s)
            .map_err(|e| ConnectionError::Underlying(format!("{:?}", e)))
    }

    /// Sets the callback for incoming messages.
    fn set_cb_message(&self, mut cb: WebRTCMessageCB) {
        let onmessage_callback = Closure::wrap(Box::new(move |ev: MessageEvent| {
            if let Some(message) = ev.data().as_string() {
                cb(message);
            }
        }) as Box<dyn FnMut(MessageEvent)>);
        self.dc
            .set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        onmessage_callback.forget();
    }

    async fn get_state(&self) -> Result<ConnectionStateMap, ConnectionError> {
        let mut csm = get_state(self.conn.clone()).await?;
        csm.data_connection = Some(self.dc.ready_state());
        Ok(csm)
    }
}

pub async fn get_state(conn: RtcPeerConnection) -> Result<ConnectionStateMap, ConnectionError> {
    let conn_stats: js_sys::Map = wasm_bindgen_futures::JsFuture::from(conn.get_stats())
        .await
        .unwrap()
        .into();

    // conn_stats.for_each(&mut |v, k| log_1(&format!("- {:?}: {:?}", k, v).into()));
    let mut type_remote = ConnType::Unknown;
    conn_stats.for_each(&mut |k, _v| {
        let s = format!("{:?}", k);
        if s.contains("candidateType\":\"srflx") {
            type_remote = ConnType::STUNServer;
        } else if s.contains("candidateType\":\"prflx") {
            type_remote = ConnType::STUNPeer;
        } else if s.contains("candidateType\":\"relay") {
            type_remote = ConnType::TURN;
        } else if s.contains("candidateType\":\"host") {
            type_remote = ConnType::Host;
        }
    });

    let signaling = match conn.signaling_state() {
        RtcSignalingState::Stable => SignalingState::Stable,
        RtcSignalingState::Closed => SignalingState::Closed,
        _ => SignalingState::Setup,
    };

    Ok(ConnectionStateMap {
        ice_gathering: conn.ice_gathering_state(),
        ice_connection: conn.ice_connection_state(),
        data_connection: None,
        signaling,
        delay_ms: 0,
        tx_bytes: 0,
        rx_bytes: 0,
        type_remote,
        type_local: type_remote,
    })
}
