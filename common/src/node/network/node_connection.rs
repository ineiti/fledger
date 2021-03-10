use crate::{
    node::{
        ext_interface::Logger,
        network::connection_state::{CSEnum, CSInput, CSOutput, ConnectionState},
    },
    signal::web_rtc::{ConnectionStateMap, PeerMessage},
};

use crate::signal::web_rtc::WebRTCSpawner;
use std::sync::{
    mpsc::{channel, Receiver, Sender},
    Arc, Mutex,
};

#[derive(Debug)]
pub enum NCInput {
    WebSocket(PeerMessage, bool),
}

#[derive(Debug)]
pub enum NCOutput {
    WebSocket(PeerMessage, bool),
    WebRTCMessage(String),
}

/// There might be up to two connections per remote node.
/// This is in the case both nodes try to set up a connection at the same time.
/// This race condition is very difficult to catch, so it's easier to just allow
/// two connections per remote node.
/// If a second, third, or later incoming connection from the same node happens, the previous
/// connection is considered stale and discarded.
pub struct NodeConnection {
    // outgoing connections are the preferred ones.
    pub outgoing: ConnectionState,
    // incoming connections are connections initiated from another node.
    pub incoming: ConnectionState,

    // channels for communicating with this module
    pub output_rx: Receiver<NCOutput>,
    pub input_tx: Sender<NCInput>,
    output_tx: Sender<NCOutput>,
    input_rx: Receiver<NCInput>,

    logger: Box<dyn Logger>,
    states: Vec<Option<ConnectionStateMap>>,
}

impl NodeConnection {
    pub fn new(logger: Box<dyn Logger>, web_rtc: Arc<Mutex<WebRTCSpawner>>) ->
    Result<NodeConnection, String> {
        let (output_tx, output_rx) = channel::<NCOutput>();
        let (input_tx, input_rx) = channel::<NCInput>();
        let nc = NodeConnection {
            outgoing: ConnectionState::new(false, logger.clone(), Arc::clone(&web_rtc))?,
            incoming: ConnectionState::new(true, logger.clone(), Arc::clone(&web_rtc))?,
            output_tx,
            output_rx,
            input_tx,
            input_rx,
            logger,
            states: vec![None, None],
        };
        Ok(nc)
    }

    /// Processes all messages waiting from the submodules, and calls the submodules to
    /// process waiting messages.
    pub async fn process(&mut self) -> Result<(), String> {
        self.process_connection(true, self.incoming.output_rx.try_iter().collect())
            .await?;
        self.process_connection(false, self.outgoing.output_rx.try_iter().collect())
            .await?;
        self.process_incoming().await?;
        self.incoming.process().await?;
        self.outgoing.process().await?;
        Ok(())
    }

    /// Tries to send a message over the webrtc connection.
    /// If the connection is in setup phase, the message is queued.
    /// If the connection is idle, an error is returned.
    pub fn send(&mut self, msg: String) -> Result<(), String> {
        // self.logger.info("dbg: Sending to node");
        match self.get_connection_channel() {
            Some(chan) => chan.send(CSInput::Send(msg)).map_err(|e| e.to_string()),
            None => self
                .outgoing
                .input_tx
                .send(CSInput::Send(msg))
                .map_err(|e| e.to_string()),
        }
    }

    /// Return the stats of outgoing / incoming connection. Every time this method
    /// is called, a new state is requested. But the returned state is the last one
    /// received.
    pub async fn get_stats(&self) -> Result<Vec<Option<ConnectionStateMap>>, String> {
        for chan in &[&self.incoming.input_tx, &self.outgoing.input_tx] {
            chan.send(CSInput::GetState).map_err(|e| e.to_string())?;
        }
        Ok(self.states.clone())
    }

    async fn process_incoming(&mut self) -> Result<(), String> {
        let msgs: Vec<NCInput> = self.input_rx.try_iter().collect();
        for msg in msgs {
            match msg {
                NCInput::WebSocket(pi, remote) => self.process_ws(pi, remote).await?,
            }
        }
        Ok(())
    }

    async fn process_ws(&mut self, ws_msg: PeerMessage, remote: bool) -> Result<(), String> {
        let msg = CSInput::ProcessPeerMessage(ws_msg);
        match remote {
            false => self.outgoing.input_tx.send(msg).map_err(|e| e.to_string()),
            true => self.incoming.input_tx.send(msg).map_err(|e| e.to_string()),
        }
    }

    /// Processes incoming messages from a connection.
    async fn process_connection(
        &mut self,
        remote: bool,
        cmds: Vec<CSOutput>,
    ) -> Result<(), String> {
        for cmd in cmds {
            // self.logger.info(&format!("dbg: NodeConnect::process {:?}", cmd));
            match cmd {
                CSOutput::State(s) => {
                    if remote {
                        self.states[1] = s;
                    } else {
                        self.states[0] = s;
                    }
                }
                CSOutput::WebSocket(msg) => self
                    .output_tx
                    .send(NCOutput::WebSocket(msg, remote))
                    .map_err(|e| e.to_string())?,
                CSOutput::WebRTCMessage(msg) => self
                    .output_tx
                    .send(NCOutput::WebRTCMessage(msg))
                    .map_err(|e| e.to_string())?,
            }
        }
        Ok(())
    }

    /// Return a connected direction, preferably outgoing.
    /// Else if one of the connections is setup, return setup (incoming first).
    /// If all else fails, return None.
    fn get_connection_channel(&mut self) -> Option<Sender<CSInput>> {
        match self.outgoing.state {
            CSEnum::Connected => return Some(self.outgoing.input_tx.clone()),
            _ => {}
        }
        match self.incoming.state {
            CSEnum::Idle => {}
            _ => return Some(self.incoming.input_tx.clone()),
        }
        match self.outgoing.state {
            CSEnum::Setup => Some(self.outgoing.input_tx.clone()),
            _ => None,
        }
    }
}
