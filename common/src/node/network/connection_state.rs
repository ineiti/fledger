use crate::{
    broker::BrokerMessage,
    node::{
        network::{ConnStats, NetworkConnectionState, NodeMessage},
        BrokerNetwork,
    },
    signal::web_rtc::{PeerInfo, WSSignalMessage},
    types::U256,
};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use web_sys::{RtcDataChannelState, RtcIceConnectionState};

use crate::{
    broker::Broker,
    signal::web_rtc::{
        ConnType, ConnectionError, ConnectionStateMap, PeerMessage, SetupError, WebRTCConnection,
        WebRTCConnectionSetup, WebRTCConnectionState, WebRTCSetupCBMessage, WebRTCSpawner,
    },
};

#[derive(Error, Debug)]
pub enum CSError {
    #[error("While using input queue")]
    InputQueue,
    #[error("While using output queue")]
    OutputQueue,
    #[error("Invalid PeerMessage")]
    InvalidPeerMessage(String),
    #[error("Internal error")]
    InternalError,
    #[error(transparent)]
    SetupWebRTC(#[from] SetupError),
    #[error(transparent)]
    ConnectionWebRTC(#[from] ConnectionError),
}

/// Represents the state of an incoming or outgoing connection.
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum CSEnum {
    /// No connection yet
    Idle,
    /// Connection is in progress, messages are being exchanged
    Setup,
    /// A DataChannel is available, but it might still be closed
    HasDataChannel,
}

#[derive(PartialEq, Debug, Clone)]
pub enum CSDir {
    Incoming,
    Outgoing,
}

impl CSDir {
    fn get_peer_info(&self, id_local: &U256, id_remote: &U256, message: PeerMessage) -> PeerInfo {
        let (id_init, id_follow) = match self {
            CSDir::Incoming => (id_remote, id_local),
            CSDir::Outgoing => (id_local, id_remote),
        };
        PeerInfo {
            id_init: id_init.clone(),
            id_follow: id_follow.clone(),
            message,
        }
    }

    fn get_conn_state(&self) -> WebRTCConnectionState {
        match self {
            CSDir::Incoming => WebRTCConnectionState::Follower,
            CSDir::Outgoing => WebRTCConnectionState::Initializer,
        }
    }
}

/// Holds all information necessary to setup and hold a connection.
pub struct ConnectionState {
    web_rtc: Arc<Mutex<WebRTCSpawner>>,
    conn_setup: Option<Box<dyn WebRTCConnectionSetup>>,
    direction: CSDir,
    broker: Broker,
    connection: Arc<Mutex<Connection>>,
}

impl ConnectionState {
    /// Creates a new ConnectionState by
    pub async fn new(
        incoming: bool,
        web_rtc: Arc<Mutex<WebRTCSpawner>>,
        broker: Broker,
        id_local: U256,
        id_remote: U256,
    ) -> Result<ConnectionState, CSError> {
        let direction = if incoming {
            CSDir::Incoming
        } else {
            CSDir::Outgoing
        };
        let mut cs = ConnectionState {
            connection: Arc::new(Mutex::new(Connection::new(
                broker.clone(),
                direction.clone(),
                id_local,
                id_remote,
            ))),
            web_rtc,
            conn_setup: None,
            direction: direction.clone(),
            broker,
        };
        if direction == CSDir::Outgoing {
            cs.process_peer_message(PeerMessage::Init).await?;
        }
        Ok(cs)
    }

    /// This is a bit awkward, as the data channel can be received, but not yet been open.
    pub async fn get_connection_open(&mut self) -> Result<bool, CSError> {
        self.get_state().await?;
        Ok(self
            .connection
            .lock()
            .expect("locking connection")
            .is_open()
            .await)
    }

    /// Returns the state of the connection, if available.
    pub async fn get_state(&mut self) -> Result<(), CSError> {
        let stat = match &self.get_connection_state() {
            CSEnum::HasDataChannel => Some(
                self.connection
                    .lock()
                    .expect("lock connection")
                    .connected
                    .as_ref()
                    .unwrap()
                    .get_state()
                    .await?,
            ),
            CSEnum::Setup => Some(self.conn_setup.as_ref().unwrap().get_state().await?),
            _ => None,
        };
        if let Some(s) = stat {
            let mut reset = match s.ice_connection {
                RtcIceConnectionState::Failed => true,
                RtcIceConnectionState::Disconnected => true,
                RtcIceConnectionState::Closed => true,
                RtcIceConnectionState::Connected => s.type_remote == ConnType::Unknown,
                _ => false,
            };
            if self.direction == CSDir::Outgoing {
                // Only check the ChannelState for outgoing connections - ingoing connections will
                // be reset by the remote peer.
                if let Some(state_dc) = s.data_connection.as_ref() {
                    if self.get_connection_state() == CSEnum::HasDataChannel {
                        reset = reset || state_dc == &RtcDataChannelState::Closed;
                        if reset {
                            warn!("State_dc is: {:?}", state_dc);
                        }
                    } else {
                        warn!(
                            "Didn't set reset in CSEnum::Setup for state_dc: {:?}",
                            state_dc
                        );
                    }
                }
            }
            if reset {
                warn!(
                    "Resetting with RtcIce: {:?} - type_remote: {:?}",
                    s.ice_connection, s.type_remote
                );
                self.start_connection(Some("Found bad ConnectionState".into()))
                    .await?;
            }
        }
        self.broker
            .enqueue_bm(BrokerMessage::Network(BrokerNetwork::ConnectionState(
                self.get_ncs(stat),
            )));
        Ok(())
    }

    /// Process message from websocket connection to setup an 'incoming' webrtc connection.
    pub async fn process_peer_message(&mut self, pi_message: PeerMessage) -> Result<(), CSError> {
        if self.conn_setup.is_none() {
            self.setup_new_connection().await?;
        }

        if self.get_connection_state() != CSEnum::Setup
            && matches!(pi_message, PeerMessage::Init | PeerMessage::Offer(_))
        {
            if self
                .connection
                .lock()
                .expect("lock connection")
                .connected
                .is_some()
            {
                warn!("Resetting connection for {:?}", pi_message);
            }
            self.setup_new_connection().await?;
        }

        let setup = self.conn_setup.as_mut().ok_or(CSError::InternalError)?;

        // log::debug!("{:?} process pi_message {}", self.direction, pi_message);
        match pi_message {
            PeerMessage::Init => {
                if self.direction == CSDir::Incoming {
                    return Err(CSError::InvalidPeerMessage(
                        "Only Initializer can initialize".into(),
                    ));
                }
                let offer = setup.make_offer().await?;
                self.connection
                    .lock()
                    .expect("get connection")
                    .send_ws(PeerMessage::Offer(offer));
            }
            PeerMessage::Offer(offer) => {
                if self.direction == CSDir::Outgoing {
                    return Err(CSError::InvalidPeerMessage(
                        "Only follower can treat offer".into(),
                    ));
                }
                let answer = setup.make_answer(offer).await?;
                self.connection
                    .lock()
                    .expect("get connection")
                    .send_ws(PeerMessage::Answer(answer));
            }
            PeerMessage::Answer(answer) => {
                if self.direction == CSDir::Incoming {
                    return Err(CSError::InvalidPeerMessage(
                        "Only initializer can treat answer".into(),
                    ));
                }
                if self.connection.lock().unwrap().state != CSEnum::Setup {
                    return Err(CSError::InvalidPeerMessage(
                        "Cannot use answer if not in CSEnum::Setup state".into(),
                    ));
                }
                setup.use_answer(answer).await?;
            }
            PeerMessage::IceCandidate(ice) => {
                if self.connection.lock().unwrap().state == CSEnum::Idle {
                    warn!("Shouldn't be getting IceCandidates when Idle");
                }
                setup.ice_put(ice).await?;
            }
        }
        Ok(())
    }

    pub async fn start_connection(&mut self, reason: Option<String>) -> Result<(), CSError> {
        if let Some(s) = reason {
            error!("{}", s);
        }
        if self.get_connection_state() != CSEnum::Idle {
            self.update_connection(CSEnum::Idle, None);
            self.broker
                .enqueue_bm(BrokerMessage::Network(BrokerNetwork::ConnectionState(
                    self.get_ncs(None),
                )));
        }
        if self.direction == CSDir::Incoming {
            error!("Cannot start incoming connection")
        } else {
            self.process_peer_message(PeerMessage::Init).await?;
        }
        Ok(())
    }

    /// Sends the message to the remote end.
    pub async fn send(&mut self, msg: String) -> Result<(), CSError> {
        self.get_state().await?;
        match self.get_connection_state() {
            CSEnum::Idle => {
                if let Err(e) = self.process_peer_message(PeerMessage::Init).await {
                    error!("Got error: {:?}", e);
                    return Err(e);
                }
            }
            CSEnum::HasDataChannel => {
                let reply = self
                    .connection
                    .lock()
                    .expect("lock connection")
                    .connected
                    .as_ref()
                    .unwrap()
                    .send(msg);
                if let Err(err) = reply {
                    error!("While sending: {:?}", err);
                    self.start_connection(Some(
                        "Couldn't send over webrtc, resetting connection".to_string(),
                    ))
                    .await?;
                } else {
                    self.get_state().await?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Sets up a new connection and sets up a callback for ICE messages and completeion of
    /// connection setup.
    async fn setup_new_connection(&mut self) -> Result<(), CSError> {
        let state = self.direction.get_conn_state();
        let mut rtc_conn = self.web_rtc.lock().unwrap()(state)?;
        let mut broker = self.broker.clone();
        let conn_cl = Arc::clone(&self.connection);
        rtc_conn
            .set_callback(Box::new(move |msg| {
                conn_cl
                    .lock()
                    .expect("Getting connection")
                    .web_rtc_setup(msg);
                // This is not really necessary, but speeds up the time for the connection
                if let Err(_) = broker.process() {
                    warn!("Couldn't lock process");
                }
            }))
            .await;
        self.broker
            .enqueue_bm(BrokerMessage::Network(BrokerNetwork::ConnectionState(
                self.get_ncs(None),
            )));
        self.conn_setup = Some(rtc_conn);
        self.update_connection(CSEnum::Setup, None);
        Ok(())
    }

    pub fn get_connection_state(&self) -> CSEnum {
        self.connection
            .lock()
            .expect("get connection")
            .state
            .clone()
    }

    fn update_connection(&self, state: CSEnum, connected: Option<Box<dyn WebRTCConnection>>) {
        let mut conn = self.connection.lock().expect("get connection");
        conn.state = state;
        conn.connected = connected;
    }

    fn get_ncs(&self, stat: Option<ConnectionStateMap>) -> NetworkConnectionState {
        self.connection
            .lock()
            .expect("get connection")
            .get_ncs(stat)
    }
}

struct Connection {
    broker: Broker,
    direction: CSDir,
    id_local: U256,
    id_remote: U256,
    state: CSEnum,
    connected: Option<Box<dyn WebRTCConnection>>,
}

impl Connection {
    fn new(broker: Broker, direction: CSDir, id_local: U256, id_remote: U256) -> Self {
        Self {
            broker,
            direction,
            id_local,
            id_remote,
            state: CSEnum::Idle,
            connected: None,
        }
    }

    pub fn web_rtc_setup(&mut self, s: WebRTCSetupCBMessage) {
        match s {
            WebRTCSetupCBMessage::Ice(ice) => {
                self.send_ws(PeerMessage::IceCandidate(ice));
            }
            WebRTCSetupCBMessage::Connection(conn) => {
                info!("Connected {:?}", self.direction);
                let mut broker = self.broker.clone();
                let id = self.id_remote.clone();
                conn.set_cb_message(Box::new(move |msg| {
                    if let Ok(msg) = serde_json::from_str(&msg) {
                        if let Err(e) =
                            broker.emit_bm(BrokerMessage::NodeMessage(NodeMessage { id, msg }))
                        {
                            error!("While emitting webrtc: {:?}", e);
                        }
                    }
                }));
                self.state = CSEnum::HasDataChannel;
                self.connected = Some(conn);
                self.broker
                    .enqueue_bm(BrokerMessage::Network(BrokerNetwork::ConnectionState(
                        self.get_ncs(None),
                    )));
            }
        }
    }

    fn send_ws(&self, message: PeerMessage) {
        let peer_info = self
            .direction
            .get_peer_info(&self.id_local, &self.id_remote, message);
        self.broker
            .enqueue_bm(BrokerMessage::Network(BrokerNetwork::WebSocket(
                WSSignalMessage::PeerSetup(peer_info),
            )));
    }

    fn get_ncs(&self, stat: Option<ConnectionStateMap>) -> NetworkConnectionState {
        NetworkConnectionState {
            id: self.id_remote.clone(),
            dir: self.direction.get_conn_state(),
            c: self.state.clone(),
            s: stat.map(|s| ConnStats {
                type_local: s.type_local,
                type_remote: s.type_remote,
                signaling: s.signaling,
                rx_bytes: s.rx_bytes,
                tx_bytes: s.tx_bytes,
                delay_ms: s.delay_ms,
            }),
        }
    }

    async fn is_open(&self) -> bool {
        if self.state == CSEnum::HasDataChannel {
            if let Some(conn) = self.connected.as_ref() {
                if let Ok(state) = conn.get_state().await {
                    if let Some(dc) = state.data_connection {
                        return dc == RtcDataChannelState::Open;
                    }
                }
            }
        }
        return false;
    }
}
