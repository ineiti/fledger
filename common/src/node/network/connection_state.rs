/// This handles one connection, either an incoming or an outgoing connection.

use log::{error, info, warn};
use backtrace::Backtrace;
use std::sync::{
    mpsc::{channel, Receiver, Sender},
    Arc, Mutex,
};
use web_sys::{RtcDataChannelState, RtcIceConnectionState};

use crate::{
    signal::web_rtc::{
        ConnType, ConnectionStateMap, PeerMessage, WebRTCConnection, WebRTCConnectionSetup,
        WebRTCConnectionState, WebRTCSetupCBMessage, WebRTCSpawner,
    },
    types::ProcessCallback,
};

/// Represents the state of an incoming or outgoing connection.
#[derive(PartialEq, Debug, Clone)]
pub enum CSEnum {
    /// No connection yet
    Idle,
    /// Connection is in progress, messages are being exchanged
    Setup,
    /// A DataChannel is available, but it might still be closed
    HasDataChannel,
}

/// Messages sent by the parent to ConnectionState.
#[derive(Debug)]
pub enum CSInput {
    GetState,
    ProcessPeerMessage(PeerMessage),
    Send(String),
    WebRTCSetup(WebRTCSetupCBMessage),
    StartConnection,
}

/// Messages from ConnectionState to the parent or other modules.
#[derive(Debug)]
pub enum CSOutput {
    State(CSEnum, Option<ConnectionStateMap>),
    WebSocket(PeerMessage),
    WebRTCMessage(String),
}

/// Holds all information necessary to setup and hold a connection.
pub struct ConnectionState {
    pub state: CSEnum,
    pub output_rx: Receiver<CSOutput>,
    pub input_tx: Sender<CSInput>,
    output_tx: Sender<CSOutput>,
    input_rx: Receiver<CSInput>,
    web_rtc: Arc<Mutex<WebRTCSpawner>>,
    setup: Option<Box<dyn WebRTCConnectionSetup>>,
    connected: Option<Box<dyn WebRTCConnection>>,
    remote: bool,
    process: ProcessCallback,
}

impl ConnectionState {
    /// Creates a new ConnectionState by
    pub fn new(
        remote: bool,
        web_rtc: Arc<Mutex<WebRTCSpawner>>,
        process: ProcessCallback,
    ) -> Result<ConnectionState, String> {
        let (output_tx, output_rx) = channel::<CSOutput>();
        let (input_tx, input_rx) = channel::<CSInput>();
        let cs = ConnectionState {
            state: CSEnum::Idle,
            output_rx,
            output_tx,
            input_rx,
            input_tx,
            web_rtc,
            setup: None,
            connected: None,
            remote,
            process,
        };
        if !remote {
            cs.input_tx
                .send(CSInput::ProcessPeerMessage(PeerMessage::Init))
                .map_err(|e| e.to_string())?;
        }
        Ok(cs)
    }

    pub async fn process(&mut self) -> Result<(), String> {
        let inputs: Vec<CSInput> = self.input_rx.try_iter().collect();
        for input in inputs {
            match input {
                CSInput::GetState => self.get_state().await?,
                CSInput::ProcessPeerMessage(msg) => self.process_peer_message(msg).await?,
                CSInput::Send(s) => self.send(s).await?,
                CSInput::WebRTCSetup(s) => self.web_rtc_setup(s)?,
                CSInput::StartConnection => {
                    self.start_connection(None).await?;
                }
            };
        }
        Ok(())
    }

    /// This is a bit awkward, as the data channel can be received, but not yet been open.
    pub async fn get_connection_open(&mut self) -> Result<bool, String> {
        self.get_state().await?;
        if self.state == CSEnum::HasDataChannel {
            if let Some(conn) = self.connected.as_ref() {
                if let Ok(state) = conn.get_state().await {
                    if let Some(dc) = state.data_connection {
                        return Ok(dc == RtcDataChannelState::Open);
                    }
                }
            }
        }
        return Ok(false);
    }

    fn web_rtc_setup(&mut self, s: WebRTCSetupCBMessage) -> Result<(), String> {
        match s {
            WebRTCSetupCBMessage::Ice(ice) => self
                .output_tx
                .send(CSOutput::WebSocket(PeerMessage::IceCandidate(ice)))
                .map_err(|e| e.to_string()),
            WebRTCSetupCBMessage::Connection(conn) => {
                info!(
                    "Connected {}",
                    if self.remote { "incoming" } else { "outgoing" }
                );
                let chan = self.output_tx.clone();
                let process = self.process.clone();
                conn.set_cb_message(Box::new(move |msg| {
                    if let Err(e) = chan.send(CSOutput::WebRTCMessage(msg)) {
                        error!("Couldn't send WebRTCMessage to node: {}", e);
                    }
                    match process.try_lock() {
                        Ok(mut p) => p(),
                        Err(_e) => warn!("Couldn't call process"),
                    }
                }));
                self.connected = Some(conn);
                self.state = CSEnum::HasDataChannel;
                self.output_tx
                    .send(CSOutput::State(self.state.clone(), None))
                    .map_err(|e| e.to_string())?;
                Ok(())
            }
        }
    }

    /// Returns the state of the connection, if available.
    async fn get_state(&mut self) -> Result<(), String> {
        let stat = match &self.state {
            CSEnum::HasDataChannel => Some(self.connected.as_ref().unwrap().get_state().await?),
            CSEnum::Setup => Some(self.setup.as_ref().unwrap().get_state().await?),
            _ => None,
        };
        if let Some(s) = stat {
            let mut reset = match s.ice_connection {
                RtcIceConnectionState::Failed => true,
                RtcIceConnectionState::Disconnected => true,
                RtcIceConnectionState::Closed => true,
                _ => false,
            } || s.type_remote == ConnType::Unknown;
            if let Some(state_dc) = s.data_connection.as_ref(){
                reset = reset || state_dc == &RtcDataChannelState::Closed;
            }
            if reset {
                self.start_connection(Some("Found bad ConnetionState".into()))
                    .await?;
            }
        }
        self.output_tx
            .send(CSOutput::State(self.state.clone(), stat))
            .map_err(|e| e.to_string())
    }

    /// Process message from websocket connection to setup an 'incoming' webrtc connection.
    async fn process_peer_message(&mut self, pi_message: PeerMessage) -> Result<(), String> {
        match &self.state {
            CSEnum::Idle => {
                if (matches!(pi_message, PeerMessage::Offer { .. }) && self.remote)
                    || (matches!(pi_message, PeerMessage::Init) && !self.remote)
                {
                    self.setup_new_connection().await?;
                    self.setup_peer_message(pi_message).await?;
                } else {
                    return Err(format!(
                        "Wrong PeerMessage {:?} for new connection with remote = {} in {:?}",
                        pi_message,
                        self.remote,
                        Backtrace::new()
                    ));
                }
            }
            _ => {
                if let Err(e) = self.setup_peer_message(pi_message).await {
                    self.start_connection(Some(format!(
                        "Couldn't set peer message, resetting connection: {}",
                        e.to_string()
                    )))
                    .await?;
                }
            }
        }
        Ok(())
    }

    async fn start_connection(&mut self, reason: Option<String>) -> Result<(), String> {
        if let Some(s) = reason {
            error!("{}", s);
        }
        if self.state != CSEnum::Idle {
            self.state = CSEnum::Idle;
            self.output_tx
                .send(CSOutput::State(self.state.clone(), None))
                .map_err(|e| e.to_string())?;
        }
        if self.remote {
            error!("Cannot start incoming connection")
        } else {
            info!("Starting new connection");
            self.setup_new_connection().await?;
            self.setup_peer_message(PeerMessage::Init).await?;
        }
        Ok(())
    }

    /// Sends the message to the remote end.
    async fn send(&mut self, msg: String) -> Result<(), String> {
        self.get_state().await?;
        match self.state {
            CSEnum::Idle => {
                self.process_peer_message(PeerMessage::Init).await?;
            }
            CSEnum::HasDataChannel => {
                if let Err(err) = self.connected.as_ref().unwrap().send(msg) {
                    error!("While sending: {:?}", err);
                    self.start_connection(Some(
                        "Couldn't send over webrtc, resetting connection".to_string(),
                    ))
                    .await?;
                    // } else {
                    //     self.get_state().await?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Sets up a new connection and sets up a callback for ICE messages and completeion of
    /// connection setup.
    async fn setup_new_connection(&mut self) -> Result<(), String> {
        let state = match self.remote {
            true => WebRTCConnectionState::Follower,
            false => WebRTCConnectionState::Initializer,
        };
        let mut conn = self.web_rtc.lock().unwrap()(state)?;
        let sender = self.input_tx.clone();
        let process = self.process.clone();
        conn.set_callback(Box::new(move |msg| {
            if let Err(e) = sender.send(CSInput::WebRTCSetup(msg)) {
                error!("Couldn't send WebRTCSetup: {}", e);
            }
            match process.try_lock() {
                Ok(mut p) => p(),
                Err(_e) => warn!("Couldn't lock process"),
            }
        }))
        .await;
        self.state = CSEnum::Setup;
        self.output_tx
            .send(CSOutput::State(self.state.clone(), None))
            .map_err(|e| e.to_string())?;
        self.setup = Some(conn);
        self.connected = None;
        Ok(())
    }

    async fn setup_peer_message(&mut self, pi_message: PeerMessage) -> Result<(), String> {
        if let Some(setup) = self.setup.as_mut() {
            match pi_message {
                PeerMessage::Init => {
                    if self.remote {
                        return Err("Only Initializer can initialize".into());
                    }
                    let offer = setup.make_offer().await?;
                    self.output_tx
                        .send(CSOutput::WebSocket(PeerMessage::Offer(offer)))
                        .map_err(|e| e.to_string())?;
                }
                PeerMessage::Offer(offer) => {
                    if !self.remote {
                        return Err("Only follower can treat offer".into());
                    }
                    let answer = setup.make_answer(offer).await?;
                    self.output_tx
                        .send(CSOutput::WebSocket(PeerMessage::Answer(answer)))
                        .map_err(|e| e.to_string())?;
                }
                PeerMessage::Answer(answer) => {
                    if self.remote {
                        return Err("Only initializer can treat answer".into());
                    }
                    setup.use_answer(answer).await?;
                }
                PeerMessage::IceCandidate(ice) => {
                    setup.ice_put(ice).await?;
                }
            }
        } else {
            return Err("setup_peer_message cannot be called without self.setup".into());
        }
        Ok(())
    }
}
