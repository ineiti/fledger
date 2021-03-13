/// This handles one connection, either an incoming or an outgoing connection.
use backtrace::Backtrace;

use crate::{
    node::ext_interface::Logger,
    signal::web_rtc::{
        ConnectionStateMap, PeerMessage, WebRTCConnection, WebRTCConnectionSetup,
        WebRTCConnectionState, WebRTCSetupCBMessage, WebRTCSpawner,
    },
};
use std::sync::{
    mpsc::{channel, Receiver, Sender},
    Arc, Mutex,
};

/// Represents the state of an incoming or outgoing connection.
#[derive(PartialEq, Debug, Clone)]
pub enum CSEnum {
    /// No connection yet
    Idle,
    /// Connection is in progress, messages are being exchanged
    Setup,
    /// Connecion is established, data can flow.
    Connected,
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
    logger: Box<dyn Logger>,
    web_rtc: Arc<Mutex<WebRTCSpawner>>,
    setup: Option<Box<dyn WebRTCConnectionSetup>>,
    connected: Option<Box<dyn WebRTCConnection>>,
    remote: bool,
}

impl ConnectionState {
    /// Creates a new ConnectionState by
    pub fn new(
        remote: bool,
        logger: Box<dyn Logger>,
        web_rtc: Arc<Mutex<WebRTCSpawner>>,
    ) -> Result<ConnectionState, String> {
        let (output_tx, output_rx) = channel::<CSOutput>();
        let (input_tx, input_rx) = channel::<CSInput>();
        let cs = ConnectionState {
            state: CSEnum::Idle,
            output_rx,
            output_tx,
            input_rx,
            input_tx,
            logger,
            web_rtc,
            setup: None,
            connected: None,
            remote,
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
            // self.logger.info(&format!("dbg: ConnectionState::processes {:?}", input));
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

    fn web_rtc_setup(&mut self, s: WebRTCSetupCBMessage) -> Result<(), String> {
        match s {
            WebRTCSetupCBMessage::Ice(ice) => self
                .output_tx
                .send(CSOutput::WebSocket(PeerMessage::IceCandidate(ice)))
                .map_err(|e| e.to_string()),
            WebRTCSetupCBMessage::Connection(conn) => {
                self.logger.info(&format!(
                    "Connected {}",
                    if self.remote { "incoming" } else { "outgoing" }
                ));
                let chan = self.output_tx.clone();
                let log = self.logger.clone();
                conn.set_cb_message(Box::new(move |msg| {
                    if let Err(e) = chan.send(CSOutput::WebRTCMessage(msg)) {
                        log.error(&format!("Couldn't send WebRTCMessage to node: {}", e));
                    }
                }));
                self.setup = None;
                self.connected = Some(conn);
                self.state = CSEnum::Connected;
                self.output_tx
                    .send(CSOutput::State(self.state.clone(), None))
                    .map_err(|e| e.to_string())?;
                Ok(())
            }
        }
    }

    /// Returns the state of the connection, if available.
    async fn get_state(&self) -> Result<(), String> {
        let stat = match &self.state {
            CSEnum::Connected => Some(self.connected.as_ref().unwrap().get_state().await?),
            _ => None,
        };
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
            self.logger.error(&s);
        }
        if self.state != CSEnum::Idle {
            self.state = CSEnum::Idle;
            self.output_tx
                .send(CSOutput::State(self.state.clone(), None))
                .map_err(|e| e.to_string())?;
        }
        if self.remote {
            self.logger.error("Cannot start incoming connection")
        } else {
            self.logger.info("Starting new connection");
            self.setup_new_connection().await?;
            self.setup_peer_message(PeerMessage::Init).await?;
        }
        Ok(())
    }

    /// Sends the message to the remote end.
    async fn send(&mut self, msg: String) -> Result<(), String> {
        match self.state {
            CSEnum::Idle => {
                self.process_peer_message(PeerMessage::Init).await?;
            }
            CSEnum::Connected => {
                if let Err(_) = self.connected.as_ref().unwrap().send(msg) {
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
    async fn setup_new_connection(&mut self) -> Result<(), String> {
        let state = match self.remote {
            true => WebRTCConnectionState::Follower,
            false => WebRTCConnectionState::Initializer,
        };
        let mut conn = self.web_rtc.lock().unwrap()(state)?;
        let sender = self.input_tx.clone();
        let log = self.logger.clone();
        conn.set_callback(Box::new(move |msg| {
            if let Err(e) = sender.send(CSInput::WebRTCSetup(msg)) {
                log.error(&format!("Couldn't send WebRTCSetup: {}", e));
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
                    setup.wait_gathering().await?;
                    setup.ice_put(ice).await?;
                }
            }
        } else {
            match pi_message {
                PeerMessage::Init => {
                    if self.remote {
                        return Err("Cannot use Init on incoming connection".into());
                    }
                    return Err("Re-initializing outgoing connection".into());
                }
                PeerMessage::Offer(o) => {
                    if self.remote {
                        self.setup_new_connection().await?;
                        self.input_tx
                            .send(CSInput::ProcessPeerMessage(PeerMessage::Offer(o)))
                            .map_err(|e| e.to_string())?;
                    } else {
                        self.logger.warn("Got Offer for outgoing connection");
                    }
                }
                PeerMessage::Answer(_) => {
                    self.logger.warn("Cannot use Answer with this connection state");
                }
                PeerMessage::IceCandidate(_) => {
                    self.logger.warn("Ignoring IceCandidate for connection");
                }
            }
        }
        Ok(())
    }
}
