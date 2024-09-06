use async_trait::async_trait;
use futures::lock::Mutex;
use js_sys::Reflect;
use log::{error, warn};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Event, MessageEvent, RtcConfiguration, RtcDataChannel, RtcDataChannelEvent,
    RtcDataChannelState, RtcIceCandidate, RtcIceCandidateInit, RtcIceConnectionState,
    RtcIceGatheringState, RtcPeerConnection, RtcPeerConnectionIceEvent, RtcSdpType,
    RtcSessionDescriptionInit, RtcSignalingState,
};

use crate::web_rtc::{
    connection::{ConnectionConfig, HostLogin}, messages::{
        ConnType, ConnectionStateMap, DataChannelState, IceConnectionState, IceGatheringState,
        PeerMessage, SetupError, SignalingState, WebRTCInput, WebRTCMessage, WebRTCOutput,
        WebRTCSpawner,
    }, node_connection::Direction
};
use flmodules::broker::{Broker, Subsystem, SubsystemHandler};

pub struct WebRTCConnectionSetup {
    pub rp_conn: RtcPeerConnection,
    rtc_data: Arc<Mutex<Option<RtcDataChannel>>>,
    broker: Broker<WebRTCMessage>,
    // While the connection is not up, queue up messages in here.
    queue: Vec<String>,
    direction: Option<Direction>,
    config: ConnectionConfig,
}

#[derive(Serialize, Deserialize)]
struct IceServer {
    urls: String,
    username: Option<String>,
    credential: Option<String>,
}

fn get_ice_server(host: HostLogin) -> IceServer {
    let username = host.login.clone().map(|l| l.user);
    let credential = host.login.clone().map(|l| l.pass);
    IceServer {
        urls: host.url,
        username,
        credential,
    }
}

impl WebRTCConnectionSetup {
    pub async fn new(
        broker: Broker<WebRTCMessage>,
        config: ConnectionConfig,
    ) -> Result<WebRTCConnectionSetup, SetupError> {
        Ok(WebRTCConnectionSetup {
            rp_conn: Self::create_rp_conn(config.clone())?,
            rtc_data: Arc::new(Mutex::new(None)),
            broker,
            queue: vec![],
            direction: None,
            config,
        })
    }

    pub fn create_rp_conn(
        connection_cfg: ConnectionConfig,
    ) -> Result<RtcPeerConnection, SetupError> {
        // If no stun server is configured, only local IPs will be sent in the browser.
        // At least the node webrtc does the correct thing...
        let config = RtcConfiguration::new();
        let mut servers_obj = vec![get_ice_server(connection_cfg.stun())];
        if let Some(turn) = connection_cfg.turn() {
            servers_obj.push(get_ice_server(turn));
        }
        let servers = serde_wasm_bindgen::to_value(&servers_obj)
            .map_err(|e| SetupError::SetupFail(e.to_string()))?;
        config.set_ice_servers(&servers);
        RtcPeerConnection::new_with_configuration(&config)
            .map_err(|e| SetupError::SetupFail(format!("PeerConnection error: {:?}", e)))
    }

    pub fn reset(&mut self) -> Result<(), SetupError> {
        let empty_callback = Closure::wrap(Box::new(move |_: MessageEvent| {
            log::warn!("Got callback after reset");
        }) as Box<dyn FnMut(MessageEvent)>);

        if let Some(rtc_data_opt) = self.rtc_data.try_lock() {
            if let Some(rtc_data) = rtc_data_opt.as_ref() {
                rtc_data.set_onmessage(Some(empty_callback.as_ref().unchecked_ref()));
                rtc_data.set_onopen(Some(empty_callback.as_ref().unchecked_ref()));
            }
        }
        self.rp_conn
            .set_onicecandidate(Some(empty_callback.as_ref().unchecked_ref()));
        self.rp_conn
            .set_ondatachannel(Some(empty_callback.as_ref().unchecked_ref()));

        empty_callback.forget();

        self.rp_conn.close();
        self.rp_conn = Self::create_rp_conn(self.config.clone())?;
        WebRTCConnectionSetup::ice_start(&self.rp_conn, self.broker.clone());
        self.direction = None;
        if let Some(mut rd) = self.rtc_data.try_lock() {
            rd.as_ref().map(|r| r.close());
            *rd = None;
        }
        Ok(())
    }

    pub fn ice_start(rp_conn: &RtcPeerConnection, broker: Broker<WebRTCMessage>) {
        let broker_cl = broker.clone();
        let onicecandidate_callback1 =
            Closure::wrap(Box::new(move |ev: RtcPeerConnectionIceEvent| {
                let mut broker = broker_cl.clone();
                if let Some(candidate) = ev.candidate() {
                    let cand = format!("{}", candidate.candidate());
                    wasm_bindgen_futures::spawn_local(async move {
                        broker
                            .emit_msg(WebRTCMessage::Output(WebRTCOutput::Setup(
                                PeerMessage::IceCandidate(cand),
                            )))
                            .err()
                            .map(|e| log::error!("While sending ICE candidate: {:?}", e));
                    });
                }
            }) as Box<dyn FnMut(RtcPeerConnectionIceEvent)>);
        rp_conn.set_onicecandidate(Some(onicecandidate_callback1.as_ref().unchecked_ref()));
        onicecandidate_callback1.forget();
        let broker_cl = broker.clone();
        let rp_conn_cl = rp_conn.clone();
        let oniceconnectionstatechange =
            Closure::wrap(Box::new(move |_: RtcPeerConnectionIceEvent| {
                let msg = match rp_conn_cl.ice_connection_state() {
                    RtcIceConnectionState::Failed | RtcIceConnectionState::Disconnected => {
                        WebRTCMessage::Output(WebRTCOutput::Disconnected)
                    }
                    _ => WebRTCMessage::Input(WebRTCInput::UpdateState),
                };
                let mut broker = broker_cl.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    broker
                        .emit_msg(msg)
                        .err()
                        .map(|e| log::error!("While sending ICE candidate: {:?}", e));
                });
            }) as Box<dyn FnMut(RtcPeerConnectionIceEvent)>);
        rp_conn.set_oniceconnectionstatechange(Some(
            oniceconnectionstatechange.as_ref().unchecked_ref(),
        ));
        oniceconnectionstatechange.forget();
    }

    // Returns the offer string that needs to be sent to the `Follower` node.
    pub async fn make_offer(&mut self) -> Result<String, SetupError> {
        if self.direction.is_some() {
            log::warn!("Resetting with offer in already opened connection");
            self.reset()?;
        };
        self.direction = Some(Direction::Outgoing);

        let dc = self.rp_conn.create_data_channel("data-channel");
        Self::dc_set_onopen(self.broker.clone(), self.rtc_data.clone(), dc);

        let co = self.rp_conn.create_offer();
        let offer = JsFuture::from(co)
            .await
            .map_err(|e| SetupError::SetupFail(format!("{:?}", e)))?;
        let offer_sdp = Reflect::get(&offer, &JsValue::from_str("sdp"))
            .map_err(|e| SetupError::SetupFail(format!("{:?}", e)))?
            .as_string()
            .unwrap();

        let offer_obj = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
        offer_obj.set_sdp(&offer_sdp);
        let sld_promise = self.rp_conn.set_local_description(&offer_obj);
        JsFuture::from(sld_promise)
            .await
            .map_err(|e| SetupError::SetupFail(format!("{:?}", e)))?;
        Ok(offer_sdp)
    }

    // Takes the offer string
    pub async fn make_answer(&mut self, offer: String) -> Result<String, SetupError> {
        if self.direction.is_some() {
            log::warn!("Resetting with offer in already opened connection");
            self.reset()?;
        };
        self.direction = Some(Direction::Incoming);

        self.dc_create_follow();

        let offer_obj = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
        offer_obj.set_sdp(&offer);
        let srd_promise = self.rp_conn.set_remote_description(&offer_obj);
        JsFuture::from(srd_promise)
            .await
            .map_err(|e| SetupError::SetupFail(e.as_string().unwrap()))?;

        let answer = match JsFuture::from(self.rp_conn.create_answer()).await {
            Ok(f) => f,
            Err(e) => {
                error!("Error answer: {:?}", e);
                return Err(SetupError::SetupFail(e.as_string().unwrap()));
            }
        };
        let answer_sdp = Reflect::get(&answer, &JsValue::from_str("sdp"))
            .map_err(|e| SetupError::SetupFail(e.as_string().unwrap()))?
            .as_string()
            .unwrap();

        let answer_obj = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
        answer_obj.set_sdp(&answer_sdp);
        let sld_promise = self.rp_conn.set_local_description(&answer_obj);
        JsFuture::from(sld_promise)
            .await
            .map_err(|e| SetupError::SetupFail(e.as_string().unwrap()))?;
        Ok(answer_sdp)
    }

    // Takes the answer string and finalizes the first part of the connection.
    pub async fn use_answer(&mut self, answer: String) -> Result<(), SetupError> {
        let dir = self
            .direction
            .clone()
            .ok_or_else(|| SetupError::SetupFail("Direction not set".to_string()))?;
        (dir == Direction::Outgoing)
            .then(|| ())
            .ok_or_else(|| SetupError::SetupFail("Should be outgoing direction".to_string()))?;

        if self.rp_conn.signaling_state() == RtcSignalingState::Stable {
            return Ok(());
        }
        let answer_obj = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
        answer_obj.set_sdp(&answer);
        let srd_promise = self.rp_conn.set_remote_description(&answer_obj);
        JsFuture::from(srd_promise)
            .await
            .map_err(|e| SetupError::SetupFail(format!("{:?}", e)))?;
        Ok(())
    }

    // Sends the ICE string to the WebRTC.
    pub async fn ice_put(&mut self, ice: String) -> Result<(), SetupError> {
        let ric_init = RtcIceCandidateInit::new(&ice);
        ric_init.set_sdp_mid(Some("0"));
        ric_init.set_sdp_m_line_index(Some(0u16));
        match RtcIceCandidate::new(&ric_init) {
            Ok(e) => {
                if let Err(err) = wasm_bindgen_futures::JsFuture::from(
                    self.rp_conn
                        .add_ice_candidate_with_opt_rtc_ice_candidate(Some(&e)),
                )
                .await
                {
                    warn!("Couldn't add ice candidate: {:?}", err);
                }
                Ok(())
            }
            Err(err) => Err(SetupError::SetupFail(format!(
                "Couldn't consume ice: {:?}",
                err
            ))),
        }
        .map_err(|js| SetupError::SetupFail(js.to_string()))
    }

    pub async fn send(&mut self, msg: String) -> Result<(), SetupError> {
        self.queue.push(msg);
        self.send_queue().await
    }

    pub async fn send_queue(&mut self) -> Result<(), SetupError> {
        let state = self.get_state().await?;
        if let Some(state) = state.data_connection {
            if state == DataChannelState::Open {
                let rtc_data = self.rtc_data.try_lock().unwrap();
                if let Some(ref mut data_channel) = rtc_data.as_ref() {
                    for msg_queue in self.queue.drain(..) {
                        data_channel
                            .send_with_str(&msg_queue)
                            .map_err(|e| SetupError::Send(format!("{e:?}")))?;
                    }
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    fn dc_set_onopen(
        broker: Broker<WebRTCMessage>,
        rtc_data: Arc<Mutex<Option<RtcDataChannel>>>,
        dc: RtcDataChannel,
    ) {
        let dc_clone = dc.clone();
        let ondatachannel_open = Closure::wrap(Box::new(move |_ev: Event| {
            let mut broker_clone = broker.clone();
            let rtc_data = Arc::clone(&rtc_data);
            let dc_clone2 = dc_clone.clone();
            wasm_bindgen_futures::spawn_local(async move {
                rtc_data.lock().await.replace(dc_clone2.clone());
                broker_clone
                    .emit_msg(WebRTCMessage::Output(WebRTCOutput::Connected))
                    .err()
                    .map(|e| log::error!("While sending connection: {:?}", e));
            });

            let broker_cl = broker.clone();
            let onmessage_callback = Closure::wrap(Box::new(move |ev: MessageEvent| {
                if let Some(message) = ev.data().as_string() {
                    let mut broker = broker_cl.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        broker
                            .emit_msg(WebRTCMessage::Output(WebRTCOutput::Text(message)))
                            .err()
                            .map(|e| log::error!("While sending message: {:?}", e));
                    });
                }
            }) as Box<dyn FnMut(MessageEvent)>);
            dc_clone.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
            onmessage_callback.forget();

            let broker_cl = broker.clone();
            let onerror_callback = Closure::wrap(Box::new(move |ev: MessageEvent| {
                let mut broker = broker_cl.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    broker
                        .emit_msg(WebRTCMessage::Output(WebRTCOutput::Error(format!(
                            "{:?}",
                            ev
                        ))))
                        .err()
                        .map(|e| log::error!("While sending message: {:?}", e));
                });
            }) as Box<dyn FnMut(MessageEvent)>);
            dc_clone.set_onclose(Some(onerror_callback.as_ref().unchecked_ref()));
            onerror_callback.forget();
        }) as Box<dyn FnMut(Event)>);
        dc.set_onopen(Some(ondatachannel_open.as_ref().unchecked_ref()));
        ondatachannel_open.forget();
    }

    fn dc_create_follow(&self) {
        let broker = self.broker.clone();
        let rtc_data = self.rtc_data.clone();
        let ondatachannel_callback = Closure::wrap(Box::new(move |ev: RtcDataChannelEvent| {
            Self::dc_set_onopen(broker.clone(), rtc_data.clone(), ev.channel());
        })
            as Box<dyn FnMut(RtcDataChannelEvent)>);
        self.rp_conn
            .set_ondatachannel(Some(ondatachannel_callback.as_ref().unchecked_ref()));
        ondatachannel_callback.forget();
    }

    pub async fn get_state(&self) -> Result<ConnectionStateMap, SetupError> {
        let stats = self.rp_conn.get_stats();
        let conn_stats: js_sys::Map = wasm_bindgen_futures::JsFuture::from(stats)
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

        let signaling = match self.rp_conn.signaling_state() {
            RtcSignalingState::Stable => SignalingState::Stable,
            RtcSignalingState::Closed => SignalingState::Closed,
            _ => SignalingState::Setup,
        };

        let ice_gathering = match self.rp_conn.ice_gathering_state() {
            RtcIceGatheringState::New => IceGatheringState::New,
            RtcIceGatheringState::Gathering => IceGatheringState::Gathering,
            RtcIceGatheringState::Complete => IceGatheringState::Complete,
            _ => IceGatheringState::New,
        };

        let ice_connection = match self.rp_conn.ice_connection_state() {
            RtcIceConnectionState::New => IceConnectionState::New,
            RtcIceConnectionState::Checking => IceConnectionState::Checking,
            RtcIceConnectionState::Connected => IceConnectionState::Connected,
            RtcIceConnectionState::Completed => IceConnectionState::Completed,
            RtcIceConnectionState::Failed => IceConnectionState::Failed,
            RtcIceConnectionState::Disconnected => IceConnectionState::Disconnected,
            RtcIceConnectionState::Closed => IceConnectionState::Closed,
            _ => IceConnectionState::New,
        };

        let mut data_connection = None;
        if let Some(rtc_data) = self.rtc_data.try_lock() {
            if let Some(rtc_data_ref) = rtc_data.as_ref() {
                data_connection = Some(match rtc_data_ref.ready_state() {
                    RtcDataChannelState::Connecting => DataChannelState::Connecting,
                    RtcDataChannelState::Open => DataChannelState::Open,
                    RtcDataChannelState::Closing => DataChannelState::Closing,
                    RtcDataChannelState::Closed => DataChannelState::Closed,
                    _ => DataChannelState::Closed,
                });
            }
        }

        Ok(ConnectionStateMap {
            ice_gathering,
            ice_connection,
            data_connection,
            signaling,
            delay_ms: 0,
            tx_bytes: 0,
            rx_bytes: 0,
            type_remote,
            type_local: type_remote,
        })
    }
}

pub struct WebRTCConnection {
    setup: WebRTCConnectionSetup,
}

impl WebRTCConnection {
    pub async fn new_box(config: ConnectionConfig) -> Result<Broker<WebRTCMessage>, SetupError> {
        let broker = Broker::new();
        let rn = WebRTCConnection {
            setup: WebRTCConnectionSetup::new(broker.clone(), config).await?,
        };
        let rp_conn = rn.setup.rp_conn.clone();

        broker
            .clone()
            .add_subsystem(Subsystem::Handler(Box::new(rn)))
            .await?;
        WebRTCConnectionSetup::ice_start(&rp_conn, broker.clone());
        Ok(broker)
    }

    async fn setup(&mut self, pm: PeerMessage) -> Result<Option<PeerMessage>, SetupError> {
        Ok(match pm {
            PeerMessage::Init => Some(PeerMessage::Offer(self.setup.make_offer().await?)),
            PeerMessage::Offer(o) => Some(PeerMessage::Answer(self.setup.make_answer(o).await?)),
            PeerMessage::Answer(a) => {
                self.setup.use_answer(a).await?;
                None
            }
            PeerMessage::IceCandidate(ice) => {
                self.setup.ice_put(ice).await?;
                None
            }
        })
    }

    async fn msg_in(&mut self, msg: WebRTCInput) -> Result<Option<WebRTCMessage>, SetupError> {
        match msg {
            WebRTCInput::Text(s) => self.setup.send(s).await?,
            WebRTCInput::Setup(s) => {
                if let Some(msg) = self.setup(s).await? {
                    return Ok(Some(WebRTCMessage::Output(WebRTCOutput::Setup(msg))));
                }
            }
            WebRTCInput::Flush => {
                self.setup.send_queue().await?;
            }
            WebRTCInput::UpdateState => {
                return Ok(Some(WebRTCMessage::Output(WebRTCOutput::State(
                    self.setup.get_state().await?,
                ))));
            }
            WebRTCInput::Reset => self.setup.reset()?,
        }
        Ok(None)
    }
}

#[async_trait(?Send)]
impl SubsystemHandler<WebRTCMessage> for WebRTCConnection {
    async fn messages(&mut self, msgs: Vec<WebRTCMessage>) -> Vec<WebRTCMessage> {
        let mut out = vec![];
        for msg in msgs {
            if let WebRTCMessage::Input(msg_in) = msg {
                match self.msg_in(msg_in).await {
                    Ok(Some(msg)) => out.push(msg),
                    Ok(None) => {}
                    Err(e) => {
                        log::warn!("Error processing message: {:?}", e);
                    }
                }
            }
        }
        out
    }
}

pub fn web_rtc_spawner(config: ConnectionConfig) -> WebRTCSpawner {
    Box::new(move || Box::new(Box::pin(WebRTCConnection::new_box(config.clone()))))
}
