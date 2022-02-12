use std::sync::mpsc;

use flnet::signal::web_rtc::{
    SetupError, WebRTCConnection, WebRTCConnectionSetup, WebRTCSetupCBMessage,
};
use flnet_libc::web_rtc_setup::WebRTCConnectionSetupLibc;
use flutils::time::wait_ms;

#[tokio::test(flavor = "multi_thread")]
async fn connection_setup() -> Result<(), SetupError> {
    let _ = env_logger::try_init();
    let mut init = WebRTCConnectionSetupLibc::new_async_stuff(true).await?;
    let (init_cb, init_rx) = SetupCB::new_rx();
    init.set_callback(Box::new(move |msg| init_cb.cb(msg)))
        .await;

    let mut follow = WebRTCConnectionSetupLibc::new_async_stuff(false).await?;
    let (follow_cb, follow_rx) = SetupCB::new_rx();
    follow
        .set_callback(Box::new(move |msg| follow_cb.cb(msg)))
        .await;

    // let offer = init.make_offer_rtc().await?;
    // log::debug!("offer: {:?}", offer.sdp);
    // let answer = follow.make_answer_rtc(offer).await?;
    // log::debug!("answer: {:?}", answer.sdp);
    // init.use_answer_rtc(answer).await?;

    let offer = init.make_offer().await?;
    log::debug!("offer: {:?}", offer);
    let answer = follow.make_answer(offer).await?;
    log::debug!("answer: {:?}", answer);
    init.use_answer(answer).await?;

    let mut init_conn: Option<Box<dyn WebRTCConnection>> = None;
    for i in 0..10 {
        log::debug!("Loop {i}");
        if let Ok(msg) = init_rx.try_recv() {
            log::debug!("init: {:?}", msg);
            match msg {
                WebRTCSetupCBMessage::Ice(ice) => {
                    log::debug!("Got ice message: {ice}");
                    follow.ice_put(ice).await?;
                }
                WebRTCSetupCBMessage::Connection(ic) => init_conn = Some(ic),
            }
        }
        if let Ok(msg) = follow_rx.try_recv() {
            match msg {
                WebRTCSetupCBMessage::Ice(ice) => {
                    log::debug!("Got ice message: {ice}");
                    follow.ice_put(ice).await?;
                }
                _ => {}
                // WebRTCSetupCBMessage::Connection(ic) => init_conn = Some(ic),
            }
        }
        wait_ms(200).await;
    }
    if let Some(ic) = init_conn {
        ic.send("Hello".to_string())
            .map_err(|e| SetupError::Underlying(e.to_string()))?;
    }
    wait_ms(1000).await;
    Ok(())
}

struct SetupCB {
    tx: mpsc::Sender<WebRTCSetupCBMessage>,
}

// pub type WebRTCSetupCB = Box<dyn FnMut(WebRTCSetupCBMessage) + Send>;

impl SetupCB {
    fn new_rx() -> (Self, mpsc::Receiver<WebRTCSetupCBMessage>) {
        let (tx, rx) = mpsc::channel();
        (Self { tx }, rx)
    }

    fn cb(&self, msg: WebRTCSetupCBMessage) {
        self.tx.send(msg).unwrap();
    }
}
