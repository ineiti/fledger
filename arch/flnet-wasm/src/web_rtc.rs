use async_trait::async_trait;
use flmodules::broker::{Broker, Destination, Subsystem, SubsystemListener};
use flnet::web_rtc::messages::{
    PeerMessage, SetupError, WebRTCInput, WebRTCMessage, WebRTCOutput, WebRTCSpawner,
};

use crate::web_rtc_setup::WebRTCConnectionSetup;

pub struct WebRTCConnection {
    setup: WebRTCConnectionSetup,
}

impl WebRTCConnection {
    pub async fn new_box() -> Result<Broker<WebRTCMessage>, SetupError> {
        let broker = Broker::new();
        let rn = WebRTCConnection {
            setup: WebRTCConnectionSetup::new(broker.clone()).await?,
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
impl SubsystemListener<WebRTCMessage> for WebRTCConnection {
    async fn messages(&mut self, msgs: Vec<WebRTCMessage>) -> Vec<(Destination, WebRTCMessage)> {
        let mut out = vec![];
        for msg in msgs {
            if let WebRTCMessage::Input(msg_in) = msg {
                match self.msg_in(msg_in).await {
                    Ok(Some(msg)) => out.push((Destination::Others, msg)),
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

pub fn web_rtc_spawner() -> WebRTCSpawner {
    Box::new(|| Box::new(Box::pin(WebRTCConnection::new_box())))
}
