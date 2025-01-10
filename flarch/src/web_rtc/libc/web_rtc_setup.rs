use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use crate::broker::{Broker, Subsystem, SubsystemHandler};
use async_trait::async_trait;
use futures::lock::Mutex;
use webrtc::{
    api::{
        interceptor_registry::register_default_interceptors, media_engine::MediaEngine, setting_engine::SettingEngine, APIBuilder
    }, data_channel::{data_channel_message::DataChannelMessage, RTCDataChannel}, ice::mdns::MulticastDnsMode, ice_transport::{
        ice_candidate::{RTCIceCandidate, RTCIceCandidateInit},
        ice_connection_state::RTCIceConnectionState,
        ice_credential_type::RTCIceCredentialType,
        ice_server::RTCIceServer,
    }, interceptor::registry::Registry, peer_connection::{
        configuration::RTCConfiguration,
        peer_connection_state::RTCPeerConnectionState,
        sdp::{sdp_type::RTCSdpType, session_description::RTCSessionDescription},
        RTCPeerConnection,
    }
};

use crate::web_rtc::{
    connection::{ConnectionConfig, HostLogin},
    messages::{
        ConnType, ConnectionStateMap, DataChannelState, PeerMessage, SetupError, SignalingState,
        WebRTCInput, WebRTCMessage, WebRTCOutput, WebRTCSpawner,
    },
    node_connection::Direction,
};

fn get_ice_server(host: HostLogin) -> RTCIceServer {
    let mut server = RTCIceServer {
        urls: vec![host.url],
        ..Default::default()
    };
    if let Some(login) = host.login {
        server.username = login.user;
        server.credential = login.pass;
        server.credential_type = RTCIceCredentialType::Password;
    }

    server
}

pub struct WebRTCConnectionSetupLibc {
    connection: RTCPeerConnection,
    rtc_data: Arc<Mutex<Option<Arc<RTCDataChannel>>>>,
    broker: Broker<WebRTCMessage>,
    // While the connection is not up, queue up messages in here.
    queue: Vec<String>,
    direction: Option<Direction>,
    resets: Arc<AtomicU32>,
    connection_cfg: ConnectionConfig,
}

impl WebRTCConnectionSetupLibc {
    pub async fn new_box(
        connection_cfg: ConnectionConfig,
    ) -> Result<Broker<WebRTCMessage>, SetupError> {
        let mut web_rtc = Box::new(WebRTCConnectionSetupLibc {
            connection: Self::make_connection(connection_cfg.clone()).await?,
            rtc_data: Arc::new(Mutex::new(None)),
            broker: Broker::new(),
            queue: vec![],
            direction: None,
            resets: Arc::new(AtomicU32::new(0)),
            connection_cfg,
        });

        web_rtc.setup_connection().await?;

        let mut broker = web_rtc.broker.clone();
        broker.add_subsystem(Subsystem::Handler(web_rtc)).await?;

        Ok(broker)
    }

    async fn make_connection(
        connection_cfg: ConnectionConfig,
    ) -> Result<RTCPeerConnection, SetupError> {
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

        // There seems to be some trouble with mdns where it can flood the local network
        // with requests - so turn it off.
        let mut setting_engine = SettingEngine::default();
        setting_engine.set_ice_multicast_dns_mode(MulticastDnsMode::Disabled);

        // Create the API object with the MediaEngine
        let api = APIBuilder::new()
            .with_media_engine(m)
            .with_interceptor_registry(registry)
            .with_setting_engine(setting_engine)
            .build();

        // Prepare the configuration
        let mut ice_servers = vec![get_ice_server(connection_cfg.stun())];
        if let Some(turn) = connection_cfg.turn() {
            ice_servers.push(get_ice_server(turn));
        }
        let config = RTCConfiguration {
            ice_servers,
            ..Default::default()
        };

        api.new_peer_connection(config).await.map_err(to_error)
    }

    async fn setup_connection(&mut self) -> Result<(), SetupError> {
        let broker_cl = self.broker.clone();
        let resets = Arc::clone(&self.resets);
        resets.fetch_add(1, Ordering::Relaxed);
        let resets_current = resets.load(Ordering::Relaxed);
        self.connection
            .on_ice_candidate(Box::new(move |ice_op: Option<RTCIceCandidate>| {
                if resets.load(Ordering::Relaxed) != resets_current {
                    log::warn!("Got message for deprecated on_ice_candidate");
                    return Box::pin(async {});
                }
                let broker_cl = broker_cl.clone();
                Box::pin(async move {
                    let mut broker_cl = broker_cl.clone();
                    if let Some(ice) = ice_op {
                        let ice_str = ice.to_json().unwrap().candidate;
                        broker_cl
                            .emit_msg(WebRTCMessage::Output(WebRTCOutput::Setup(
                                PeerMessage::IceCandidate(ice_str),
                            )))
                            .err()
                            .map(|e| log::warn!("Ice candidate queued but not processed: {:?}", e));
                    }
                })
            }));

        let broker_cl = self.broker.clone();
        let resets = Arc::clone(&self.resets);
        self.connection.on_peer_connection_state_change(Box::new(
            move |s: RTCPeerConnectionState| {
                log::trace!("Peer Connection State has changed: {}", s);
                if resets.load(Ordering::Relaxed) != resets_current {
                    log::warn!("Got message for deprecated on_connection_state");
                    return Box::pin(async {});
                }

                let mut broker_cl = broker_cl.clone();
                Box::pin(async move {
                    let msg = match s {
                        RTCPeerConnectionState::Disconnected
                        | RTCPeerConnectionState::Failed
                        | RTCPeerConnectionState::Closed => {
                            WebRTCMessage::Output(WebRTCOutput::Disconnected)
                        }
                        _ => WebRTCMessage::Input(WebRTCInput::UpdateState),
                    };
                    broker_cl
                        .emit_msg(msg)
                        .err()
                        .map(|e| log::warn!("UpdateState queued but not processed: {:?}", e));
                })
            },
        ));

        Ok(())
    }

    fn get_desc(sdp_type: RTCSdpType, sdp: String) -> RTCSessionDescription {
        let sdp_conv = sdp.replace("\r\n", "\\r\\n");
        let rtcsession = &format!("{{ \"type\": \"{sdp_type}\", \"sdp\": \"{sdp_conv}\"}}");
        let rtcsess = serde_json::from_str(rtcsession).unwrap();
        rtcsess
    }

    /// Returns the offer string that needs to be sent to the `Follower` node.
    async fn make_offer(&mut self) -> Result<String, SetupError> {
        if self.direction.is_some() {
            self.reset().await?;
        }
        self.direction = Some(Direction::Outgoing);
        let data_channel = self
            .connection
            .create_data_channel("data", None)
            .await
            .map_err(to_error)?;

        Self::register_data_channel(
            Arc::clone(&self.rtc_data),
            data_channel,
            self.broker.clone(),
            Arc::clone(&self.resets),
        )
        .await;
        let offer = self.connection.create_offer(None).await.map_err(to_error)?;
        self.connection
            .set_local_description(offer.clone())
            .await
            .map_err(to_error)?;

        Ok(offer.sdp)
    }

    /// Takes the offer string
    async fn make_answer(&mut self, offer: String) -> Result<String, SetupError> {
        if self.direction.is_some() {
            self.reset().await?;
        }
        self.direction = Some(Direction::Incoming);

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

        // Register data channel creation handling
        let rtc_data = Arc::clone(&self.rtc_data);
        let broker = self.broker.clone();
        let resets = Arc::clone(&self.resets);
        let resets_current = resets.load(Ordering::Relaxed);
        self.connection
            .on_data_channel(Box::new(move |rdc: Arc<RTCDataChannel>| {
                if resets.load(Ordering::Relaxed) != resets_current {
                    log::warn!("Got message for deprecated on_data_channel");
                    return Box::pin(async {});
                }

                log::trace!("New DataChannel {} {}", rdc.label(), rdc.id());
                let rtc_data = Arc::clone(&rtc_data.clone());
                // Register channel opening handling
                let broker = broker.clone();
                let resets_cl = Arc::clone(&resets);
                Box::pin(async move {
                    Self::register_data_channel(rtc_data, rdc, broker, resets_cl).await;
                })
            }));

        Ok(answer.sdp)
    }

    /// Takes the answer string and finalizes the first part of the connection.
    async fn use_answer(&mut self, answer: String) -> Result<(), SetupError> {
        match self.direction.as_ref() {
            Some(dir) => {
                if dir == &Direction::Incoming {
                    return Err(SetupError::SetupFail("direction mixup".into()));
                }
            }
            None => return Err(SetupError::SetupFail("should be connected".into())),
        }

        self.connection
            .set_remote_description(Self::get_desc(RTCSdpType::Answer, answer))
            .await
            .map_err(to_error)?;
        Ok(())
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
        let data_connection = Some(match self.connection.ice_connection_state() {
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
        });
        Ok(ConnectionStateMap {
            type_local: ConnType::Unknown,
            type_remote: ConnType::Unknown,
            signaling,
            ice_gathering: crate::web_rtc::messages::IceGatheringState::New,
            ice_connection: crate::web_rtc::messages::IceConnectionState::New,
            data_connection,
            rx_bytes: 0,
            tx_bytes: 0,
            delay_ms: 0,
        })
    }

    async fn send(&mut self, msg: String) -> Result<(), SetupError> {
        self.queue.push(msg);
        self.send_queue().await
    }

    async fn send_queue(&mut self) -> Result<(), SetupError> {
        let state_open = self.get_state().await?.data_connection == Some(DataChannelState::Open);
        if state_open || self.direction == Some(Direction::Incoming) {
            let rtc_data = self.rtc_data.lock().await;
            if let Some(ref mut data_channel) = rtc_data.as_ref() {
                for msg_queue in self.queue.drain(..) {
                    data_channel
                        .send_text(msg_queue)
                        .await
                        .map_err(|e| SetupError::Send(e.to_string()))?;
                }
                return Ok(());
            }
        }
        Ok(())
    }

    async fn setup(&mut self, pm: PeerMessage) -> Result<Option<PeerMessage>, SetupError> {
        Ok(match pm {
            PeerMessage::Init => Some(PeerMessage::Offer(self.make_offer().await?)),
            PeerMessage::Offer(o) => Some(PeerMessage::Answer(self.make_answer(o).await?)),
            PeerMessage::Answer(a) => {
                self.use_answer(a).await?;
                None
            }
            PeerMessage::IceCandidate(ice) => {
                self.ice_put(ice).await?;
                None
            }
        })
    }

    async fn register_data_channel(
        rtc_data: Arc<Mutex<Option<Arc<RTCDataChannel>>>>,
        data_channel: Arc<RTCDataChannel>,
        broker: Broker<WebRTCMessage>,
        resets: Arc<AtomicU32>,
    ) {
        let mut broker_cl = broker.clone();
        let resets_current = resets.load(Ordering::Relaxed);
        let resets_cl = Arc::clone(&resets);
        data_channel.on_open(Box::new(move || {
            if resets_cl.load(Ordering::Relaxed) != resets_current {
                log::warn!("Got message for deprecated on_open");
                return Box::pin(async {});
            }

            log::trace!("DataChannel is opened");
            Box::pin(async move {
                broker_cl
                    .emit_msg(WebRTCMessage::Output(WebRTCOutput::Connected))
                    .err()
                    .map(|e| log::warn!("Connected queued but not processed: {:?}", e));
                broker_cl
                    .emit_msg(WebRTCMessage::Input(WebRTCInput::Flush))
                    .err()
                    .map(|e| log::warn!("Flush queued but not processed: {:?}", e));
            })
        }));
        data_channel.on_message(Box::new(move |msg: DataChannelMessage| {
            if resets.load(Ordering::Relaxed) != resets_current {
                log::warn!("Got message for deprecated on_message");
                return Box::pin(async {});
            }
            let msg_str = String::from_utf8(msg.data.to_vec()).unwrap();
            let mut broker = broker.clone();
            Box::pin(async move {
                broker
                    .emit_msg(WebRTCMessage::Output(WebRTCOutput::Text(msg_str)))
                    .err()
                    .map(|e| log::warn!("Text queued but not processed: {:?}", e));
            })
        }));
        if let Some(dc) = rtc_data.lock().await.take() {
            if let Err(e) = dc.close().await {
                log::warn!("While closing datachannel: {e:?}");
            }
        }
        rtc_data.lock().await.replace(data_channel);
    }

    async fn msg_in(&mut self, msg: WebRTCInput) -> Result<Option<WebRTCMessage>, SetupError> {
        match msg {
            WebRTCInput::Text(s) => self.send(s).await?,
            WebRTCInput::Setup(s) => {
                if let Some(msg) = self.setup(s).await? {
                    return Ok(Some(WebRTCMessage::Output(WebRTCOutput::Setup(msg))));
                }
            }
            WebRTCInput::Flush => {
                self.send_queue().await?;
            }
            WebRTCInput::UpdateState => {
                return Ok(Some(WebRTCMessage::Output(WebRTCOutput::State(
                    self.get_state().await?,
                ))));
            }
            WebRTCInput::Disconnect => {
                if let Err(e) = self.reset().await {
                    log::warn!("While closing old connection: {e:?}");
                }
            }
            WebRTCInput::Reset => {
                if self.direction.is_some() {
                    self.reset().await?;
                }
            }
        }
        Ok(None)
    }

    async fn reset(&mut self) -> Result<(), SetupError> {
        if self.direction.is_none() {
            return Ok(());
        }
        self.direction = None;

        // Replacing all listeners with empty listeners
        if let Some(mut rd) = self.rtc_data.try_lock() {
            if let Some(ref mut dc) = rd.as_ref() {
                dc.on_message(Box::new(|_: DataChannelMessage| Box::pin(async {})));
                dc.on_open(Box::new(|| Box::pin(async {})));
            }
            *rd = None;
        }
        self.connection
            .on_data_channel(Box::new(|_: Arc<RTCDataChannel>| Box::pin(async {})));
        self.connection
            .on_peer_connection_state_change(Box::new(|_: RTCPeerConnectionState| {
                Box::pin(async {})
            }));
        self.connection
            .on_ice_candidate(Box::new(|_: Option<RTCIceCandidate>| Box::pin(async {})));

        if let Err(e) = self.connection.close().await {
            log::warn!("While closing old connection: {e:?}");
        }

        self.connection = Self::make_connection(self.connection_cfg.clone()).await?;
        self.setup_connection().await?;
        Ok(())
    }
}

#[async_trait]
impl SubsystemHandler<WebRTCMessage> for WebRTCConnectionSetupLibc {
    async fn messages(&mut self, msgs: Vec<WebRTCMessage>) -> Vec<WebRTCMessage> {
        let mut out = vec![];
        for msg in msgs {
            if let WebRTCMessage::Input(msg_in) = msg {
                match self.msg_in(msg_in.clone()).await {
                    Ok(Some(msg)) => out.push(msg),
                    Ok(None) => {}
                    Err(e) => {
                        log::trace!("{:p} Error processing message {msg_in:?}: {:?}", self, e);
                    }
                }
            }
        }
        out
    }
}

fn to_error(e: webrtc::error::Error) -> SetupError {
    SetupError::SetupFail(e.to_string())
}

pub fn web_rtc_spawner(config: ConnectionConfig) -> WebRTCSpawner {
    Box::new(move || Box::new(Box::pin(WebRTCConnectionSetupLibc::new_box(config.clone()))))
}
