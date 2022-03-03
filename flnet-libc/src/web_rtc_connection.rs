use async_trait::async_trait;
use futures::executor;
use std::sync::{Arc, Mutex};
use webrtc::data_channel::{data_channel_message::DataChannelMessage, RTCDataChannel};

use flnet::signal::web_rtc::{
    ConnectionError, ConnectionStateMap, WebRTCConnection, WebRTCMessageCB, ConnType, SignalingState,
};

pub struct WebRTCConnectionLibc {
    rdc: Arc<RTCDataChannel>,
    cb: Arc<Mutex<Option<WebRTCMessageCB>>>,
}

impl WebRTCConnectionLibc {
    pub async fn new(rdc: Arc<RTCDataChannel>) -> Self {
        let web = Self {
            rdc: Arc::clone(&rdc),
            cb: Arc::new(Mutex::new(None)),
        };

        let cb_clone = Arc::clone(&web.cb);
        // Register text message handling
        rdc.on_message(Box::new(move |msg: DataChannelMessage| {
            let msg_str = String::from_utf8(msg.data.to_vec()).unwrap();
            log::debug!("Message from DataChannel: '{}'", msg_str);
            if let Some(cb) = cb_clone.lock().unwrap().as_mut() {
                cb(msg_str);
            } else {
                log::error!("No cb defined");
            }
            Box::pin(async {})
        }))
        .await;
        web
    }
}

#[async_trait(?Send)]
impl WebRTCConnection for WebRTCConnectionLibc {
    fn send(&self, s: String) -> Result<(), ConnectionError> {
        executor::block_on(async { self.rdc.send_text(s).await })
            .map_err(|e| ConnectionError::FailSend(e.to_string()))?;
        Ok(())
    }

    fn set_cb_message(&self, cb: WebRTCMessageCB) {
        self.cb.lock().unwrap().replace(cb);
    }

    async fn get_state(&self) -> Result<ConnectionStateMap, ConnectionError> {
        Ok(ConnectionStateMap {
            type_local: ConnType::Unknown,
            type_remote: ConnType::Unknown,
            signaling: SignalingState::Setup,
            ice_gathering: flnet::signal::web_rtc::IceGatheringState::New,
            ice_connection: flnet::signal::web_rtc::IceConnectionState::New,
            data_connection: None,
            rx_bytes: 0,
            tx_bytes: 0,
            delay_ms: 0,
        })
    }
}
