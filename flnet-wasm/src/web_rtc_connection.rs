use async_trait::async_trait;

use wasm_bindgen::{prelude::Closure, JsCast};
use web_sys::{
    MessageEvent, RtcDataChannel, RtcDataChannelState, RtcPeerConnection, RtcSignalingState, RtcIceGatheringState, RtcIceConnectionState,
};

use flnet::signal::web_rtc::{
    ConnType, ConnectionError, ConnectionStateMap, DataChannelState, SignalingState,
    WebRTCConnection, WebRTCMessageCB, IceGatheringState, IceConnectionState,
};

pub struct WebRTCConnectionWasm {
    dc: RtcDataChannel,
    conn: RtcPeerConnection,
}

// Because in WASM there are no threads anyway, this should not be a problem.
#[allow(clippy::all)]
unsafe impl Send for WebRTCConnectionWasm {}

impl WebRTCConnectionWasm {
    pub fn new_box(dc: RtcDataChannel, conn: RtcPeerConnection) -> Box<dyn WebRTCConnection + Send> {
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
        csm.data_connection = Some(match self.dc.ready_state() {
            RtcDataChannelState::Connecting => DataChannelState::Connecting,
            RtcDataChannelState::Open => DataChannelState::Open,
            RtcDataChannelState::Closing => DataChannelState::Closing,
            RtcDataChannelState::Closed => DataChannelState::Closed,
            RtcDataChannelState::__Nonexhaustive => DataChannelState::Closed,
        });
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

    let ice_gathering = match conn.ice_gathering_state(){
        RtcIceGatheringState::New => IceGatheringState::New,
        RtcIceGatheringState::Gathering => IceGatheringState::Gathering,
        RtcIceGatheringState::Complete => IceGatheringState::Complete,
        RtcIceGatheringState::__Nonexhaustive => IceGatheringState::New,
    };

    let ice_connection = match conn.ice_connection_state(){
        RtcIceConnectionState::New => IceConnectionState::New,
        RtcIceConnectionState::Checking => IceConnectionState::Checking,
        RtcIceConnectionState::Connected => IceConnectionState::Connected,
        RtcIceConnectionState::Completed => IceConnectionState::Completed,
        RtcIceConnectionState::Failed => IceConnectionState::Failed,
        RtcIceConnectionState::Disconnected => IceConnectionState::Disconnected,
        RtcIceConnectionState::Closed => IceConnectionState::Closed,
        RtcIceConnectionState::__Nonexhaustive => IceConnectionState::New,
    };

    Ok(ConnectionStateMap {
        ice_gathering,
        ice_connection,
        data_connection: None,
        signaling,
        delay_ms: 0,
        tx_bytes: 0,
        rx_bytes: 0,
        type_remote,
        type_local: type_remote,
    })
}
