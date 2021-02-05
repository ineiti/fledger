

use crate::ext_interface::Logger;use super::{PeerMessage, WebRTCConnection, WebRTCConnectionSetup, WebRTCConnectionState};
use std::sync::{Arc, Mutex};

pub enum ProcessResult {
    Message(PeerMessage),
    Connection(Box<dyn WebRTCConnection>),
}

pub struct WebRTCSetup {
    setup: Arc<Mutex<Box<dyn WebRTCConnectionSetup>>>,
    mode: WebRTCConnectionState,
    logger: Box<dyn Logger>,
}

impl WebRTCSetup {
    pub fn new(
        setup: Arc<Mutex<Box<dyn WebRTCConnectionSetup>>>,
        mode: WebRTCConnectionState,
        logger: Box<dyn Logger>,
    ) -> WebRTCSetup {
        WebRTCSetup { setup, mode, logger }
    }

    /// Process treats each incoming message by updating the WebRTCConnectionSetup
    /// trait.
    /// It either returns the next message to be sent, or the connection.
    /// When the initializer returns the connection, the caller must make sure to
    /// send the PeerMessage::Done to the follower, so that it also creates the
    /// connection.
    pub async fn process(&mut self, pm: PeerMessage) -> Result<ProcessResult, String> {
        self.logger.info(&format!("Processing {:?}", pm));
        let mut setup = self.setup.lock().unwrap();
        match pm {
            PeerMessage::Init => {
                self.assert_init("Only Initializer can initialize")?;
                let offer = setup.make_offer().await?;
                Ok(ProcessResult::Message(PeerMessage::Offer(offer)))
            }
            PeerMessage::Offer(offer) => {
                self.assert_follow("Only follower can treat offer")?;
                let answer = setup.make_answer(offer).await?;
                Ok(ProcessResult::Message(PeerMessage::Answer(answer)))
            }
            PeerMessage::Answer(answer) => {
                self.assert_init("Only initializer can treat answer")?;
                setup.use_answer(answer).await?;
                setup.wait_gathering().await?;
                let ice = setup.ice_string().await?;
                Ok(ProcessResult::Message(PeerMessage::IceInit(ice)))
            }
            PeerMessage::IceInit(ice) => {
                self.assert_follow("Only follower can treat IceInit")?;
                setup.wait_gathering().await?;
                setup.ice_put(ice).await?;
                let ice = setup.ice_string().await?;
                Ok(ProcessResult::Message(PeerMessage::IceFollow(ice)))
            }
            PeerMessage::IceFollow(ice) => {
                self.assert_init("Only initializer can treat IceFollow")?;
                setup.ice_put(ice).await?;
                let conn = setup.get_connection().await?;
                Ok(ProcessResult::Connection(conn))
            }
            PeerMessage::DoneInit => {
                self.assert_follow("Only follower can treat DoneInit")?;
                let conn = setup.get_connection().await?;
                Ok(ProcessResult::Connection(conn))
            }
            PeerMessage::DoneFollow => {
                Err("Cannot treat DoneFollow".to_string())
            }
        }
    }

    fn assert(&self, err: &str, state: WebRTCConnectionState) -> Result<(), String> {
        match self.mode == state {
            true => Ok(()),
            false => Err(err.to_string()),
        }
    }

    fn assert_init(&self, err: &str) -> Result<(), String> {
        self.assert(err, WebRTCConnectionState::Initializer)
    }

    fn assert_follow(&self, err: &str) -> Result<(), String> {
        self.assert(err, WebRTCConnectionState::Follower)
    }
}
