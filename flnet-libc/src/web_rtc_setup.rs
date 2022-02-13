use std::sync::{Arc, Mutex};

use webrtc::{
    api::{
        interceptor_registry::register_default_interceptors, media_engine::MediaEngine, APIBuilder,
    },
    data_channel::RTCDataChannel,
    ice_transport::{
        ice_candidate::{RTCIceCandidate, RTCIceCandidateInit},
        ice_connection_state::RTCIceConnectionState,
        ice_server::RTCIceServer,
    },
    interceptor::registry::Registry,
    peer_connection::{
        configuration::RTCConfiguration,
        peer_connection_state::RTCPeerConnectionState,
        sdp::{sdp_type::RTCSdpType, session_description::RTCSessionDescription},
        RTCPeerConnection,
    },
};

use flnet::signal::web_rtc::{
    ConnType, ConnectionStateMap, DataChannelState, SetupError, SignalingState,
    WebRTCConnectionSetup, WebRTCSetupCB, WebRTCSetupCBMessage,
};

use crate::web_rtc_connection::WebRTCConnectionLibc;

pub struct WebRTCConnectionSetupLibc {
    callback: Arc<Mutex<Option<WebRTCSetupCB>>>,
    connection: Arc<RTCPeerConnection>,
}

impl WebRTCConnectionSetupLibc {
    pub async fn new_async_stuff(init: bool) -> Result<WebRTCConnectionSetupLibc, SetupError> {
        // Create a MediaEngine object to configure the supported codec
        let mut m = MediaEngine::default();

        // Register default codecs
        m.register_default_codecs().map_err(to_error)?;

        // Create a InterceptorRegistry. This is the user configurable RTP/RTCP Pipeline.
        // This provides NACKs, RTCP Reports and other features. If you use `webrtc.NewPeerConnection`
        // this is enabled by default. If you are manually managing You MUST create a InterceptorRegistry
        // for each PeerConnection.
        let mut registry = Registry::new();

        // Use the default set of Interceptors
        registry = register_default_interceptors(registry, &mut m).map_err(to_error)?;

        // Create the API object with the MediaEngine
        let api = APIBuilder::new()
            .with_media_engine(m)
            .with_interceptor_registry(registry)
            .build();

        // Prepare the configuration
        let config = RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_owned()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let web_rtc = WebRTCConnectionSetupLibc {
            callback: Arc::new(Mutex::new(None)),
            connection: Arc::new(api.new_peer_connection(config).await.map_err(to_error)?),
        };

        if !init {
            // Register data channel creation handling
            let cb = Arc::clone(&web_rtc.callback);
            web_rtc
                .connection
                .on_data_channel(Box::new(move |rdc: Arc<RTCDataChannel>| {
                    log::debug!("New DataChannel {} {}", rdc.label(), rdc.id());
                    let cb = Arc::clone(&cb);

                    // Register channel opening handling
                    Box::pin(async move {
                        let d2 = Arc::clone(&rdc);
                        rdc.on_open(Box::new(move || {
                            log::trace!("Data channel open.");

                            Box::pin(async move {
                                let web_libc = WebRTCConnectionLibc::new(d2).await;
                                if let Some(cb) = cb.lock().unwrap().as_mut() {
                                    cb(WebRTCSetupCBMessage::Connection(Box::new(web_libc)));
                                } else {
                                    log::error!("No callback set");
                                }
                            })
                        }))
                        .await;
                    })
                }))
                .await;
        }

        let cb = Arc::clone(&web_rtc.callback);
        web_rtc
            .connection
            .on_ice_candidate(Box::new(move |ice_op: Option<RTCIceCandidate>| {
                log::debug!("Got ice-candidate {:?}", ice_op);
                let cb = Arc::clone(&cb);
                Box::pin(async move {
                    if let Some(ice) = ice_op {
                        let ice_str = ice.to_json().await.unwrap().candidate;
                        if let Some(cb) = cb.lock().unwrap().as_mut() {
                            cb(WebRTCSetupCBMessage::Ice(ice_str));
                        } else {
                            log::error!("No callback set");
                        }
                    }
                })
            }))
            .await;

        web_rtc
            .connection
            .on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
                log::debug!("Peer Connection State has changed: {}", s);

                if s == RTCPeerConnectionState::Failed {
                    // Wait until PeerConnection has had no network activity for 30 seconds or another failure. It may be reconnected using an ICE Restart.
                    // Use webrtc.PeerConnectionStateDisconnected if you are interested in detecting faster timeout.
                    // Note that the PeerConnection may come back from PeerConnectionStateDisconnected.
                    log::error!("Peer Connection has gone to failed exiting");
                }

                Box::pin(async {})
            }))
            .await;

        Ok(web_rtc)
    }

    fn get_desc(sdp_type: RTCSdpType, sdp: String) -> RTCSessionDescription {
        let sdp_conv = sdp.replace("\r\n", "\\r\\n");
        let rtcsession = &format!("{{ \"type\": \"{sdp_type}\", \"sdp\": \"{sdp_conv}\"}}");
        let rtcsess = serde_json::from_str(rtcsession).unwrap();
        rtcsess
    }
}

fn to_error(e: webrtc::error::Error) -> SetupError {
    SetupError::SetupFail(e.to_string())
}

#[async_trait::async_trait(?Send)]
impl WebRTCConnectionSetup for WebRTCConnectionSetupLibc {
    /// Returns the offer string that needs to be sent to the `Follower` node.
    async fn make_offer(&mut self) -> Result<String, SetupError> {
        let data_channel = self
            .connection
            .create_data_channel("data", None)
            .await
            .map_err(to_error)?;

        data_channel
            .on_open(Box::new(move || {
                log::debug!("Data channel open");

                Box::pin(async move {})
            }))
            .await;
        let web_libc = WebRTCConnectionLibc::new(data_channel).await;
        if let Some(cb) = self.callback.lock().unwrap().as_mut() {
            cb(WebRTCSetupCBMessage::Connection(Box::new(web_libc)));
        } else {
            log::error!("No callback set");
        }

        let offer = self.connection.create_offer(None).await.map_err(to_error)?;
        self.connection
            .set_local_description(offer.clone())
            .await
            .map_err(to_error)?;
        Ok(offer.sdp)
    }

    /// Takes the offer string
    async fn make_answer(&mut self, offer: String) -> Result<String, SetupError> {
        let desc = Self::get_desc(RTCSdpType::Offer, offer);
        self.connection
            .set_remote_description(desc)
            .await
            .map_err(to_error)?;
        let answer = self
            .connection
            .create_answer(None)
            .await
            .map_err(to_error)?;
        self.connection
            .set_local_description(answer.clone())
            .await
            .map_err(to_error)?;
        Ok(answer.sdp)
    }

    /// Takes the answer string and finalizes the first part of the connection.
    async fn use_answer(&mut self, answer: String) -> Result<(), SetupError> {
        self.connection
            .set_remote_description(Self::get_desc(RTCSdpType::Answer, answer))
            .await
            .map_err(to_error)?;
        Ok(())
    }

    /// Returns either an ice string or a connection
    async fn set_callback(&mut self, cb: WebRTCSetupCB) {
        self.callback.lock().unwrap().replace(cb);
    }

    /// Sends the ICE string to the WebRTC.
    async fn ice_put(&mut self, ice: String) -> Result<(), SetupError> {
        self.connection
            .add_ice_candidate(RTCIceCandidateInit {
                candidate: ice,
                ..Default::default()
            })
            .await
            .map_err(to_error)?;
        Ok(())
    }

    /// Return some statistics of the connection
    async fn get_state(&self) -> Result<ConnectionStateMap, SetupError> {
        let signaling = match self.connection.connection_state() {
            RTCPeerConnectionState::Failed
            | RTCPeerConnectionState::Closed
            | RTCPeerConnectionState::Disconnected
            | RTCPeerConnectionState::Unspecified => SignalingState::Closed,
            RTCPeerConnectionState::New | RTCPeerConnectionState::Connecting => {
                SignalingState::Setup
            }
            RTCPeerConnectionState::Connected => SignalingState::Stable,
        };
        let data_connection = match self.connection.ice_connection_state() {
            RTCIceConnectionState::New | RTCIceConnectionState::Checking => {
                DataChannelState::Connecting
            }
            RTCIceConnectionState::Connected | RTCIceConnectionState::Completed => {
                DataChannelState::Open
            }
            RTCIceConnectionState::Unspecified => DataChannelState::Closing,
            RTCIceConnectionState::Disconnected
            | RTCIceConnectionState::Failed
            | RTCIceConnectionState::Closed => DataChannelState::Closed,
        };
        Ok(ConnectionStateMap {
            type_local: ConnType::Unknown,
            type_remote: ConnType::Unknown,
            signaling,
            ice_gathering: flnet::signal::web_rtc::IceGatheringState::New,
            ice_connection: flnet::signal::web_rtc::IceConnectionState::New,
            data_connection: Some(data_connection),
            rx_bytes: 0,
            tx_bytes: 0,
            delay_ms: 0,
        })
    }
}
