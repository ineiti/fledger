use std::sync::mpsc;

use flnet::signal::web_rtc::{
    SetupError, WebRTCConnection, WebRTCConnectionSetup, WebRTCSetupCBMessage, WebRTCConnectionState,
};
use flnet_libc::web_rtc_setup::WebRTCConnectionSetupLibc;
use flutils::time::wait_ms;

#[tokio::test(flavor = "multi_thread")]
async fn connection_setup() -> Result<(), SetupError> {
    let _ = env_logger::try_init();

    let mut init = Node::new(WebRTCConnectionState::Initializer).await;
    let mut follow = Node::new(WebRTCConnectionState::Follower).await;
    log::debug!("Started init and follow node");

    let offer = init.setup.make_offer().await?;
    log::trace!("offer: {:?}", offer);
    let answer = follow.setup.make_answer(offer).await?;
    log::trace!("answer: {:?}", answer);
    init.setup.use_answer(answer).await?;

    for i in 0..10 {
        log::debug!("Loop {i}");
        init.poll(&mut follow).await?;
        follow.poll(&mut init).await?;
        wait_ms(200).await;
    }

    log::debug!("Connection should be established");
    init.send("init->follow")?;
    follow.send("follow->init")?;
    wait_ms(1000).await;

    assert_eq!(init.rcv()?, String::from("follow->init"));
    assert_eq!(follow.rcv()?, String::from("init->follow"));
    Ok(())
}

struct Node {
    rx: mpsc::Receiver<WebRTCSetupCBMessage>,
    msg_rx: Option<mpsc::Receiver<String>>,
    setup: Box<dyn WebRTCConnectionSetup>,
    conn: Option<Box<dyn WebRTCConnection>>,
}

impl Node {
    async fn new(conn: WebRTCConnectionState) -> Self {
        let mut setup = WebRTCConnectionSetupLibc::new_box_async(conn)
            .await
            .unwrap();
        let (tx, rx) = mpsc::channel();
        setup
            .set_callback(Box::new(move |msg| tx.send(msg).unwrap()))
            .await;
        Self {
            rx,
            msg_rx: None,
            setup,
            conn: None,
        }
    }

    async fn poll(&mut self, remote: &mut Node) -> Result<(), SetupError> {
        if let Ok(msg) = self.rx.try_recv() {
            log::debug!("poll: {:?}", msg);
            match msg {
                WebRTCSetupCBMessage::Ice(ice) => {
                    log::trace!("Got ice message: {ice}");
                    remote.setup.ice_put(ice).await?;
                }
                WebRTCSetupCBMessage::Connection(ic) => {
                    let (msg_tx, msg_rx) = mpsc::channel();
                    ic.set_cb_message(Box::new(move |msg| {
                        msg_tx.send(msg).unwrap();
                    }));
                    self.conn = Some(ic);
                    self.msg_rx = Some(msg_rx);
                }
            }
        }
        Ok(())
    }

    fn send(&self, msg: &str) -> Result<(), SetupError> {
        if let Some(ic) = self.conn.as_ref() {
            return ic.send(msg.to_string())
                .map_err(|e| SetupError::Underlying(e.to_string()))
        }
        Err(SetupError::Underlying("No connection established in init".to_string()))
    }

    fn rcv(&self) -> Result<String, SetupError> {
        if let Some(msg_rx) = &self.msg_rx {
            msg_rx
                .recv()
                .map_err(|e| SetupError::Underlying(e.to_string()))
        } else {
            Err(SetupError::Underlying("No receiving channel".to_string()))
        }
    }
}
