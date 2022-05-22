use flnet::web_rtc::messages::{PeerMessage, SetupError, WebRTCMessage, WebRTCOutput, WebRTCInput};
use flnet_libc::web_rtc_setup::WebRTCConnectionSetupLibc;
use flarch::{
    tasks::wait_ms,
};
use flmodules::broker::{Broker, Destination};
use futures::channel::oneshot;

fn link_webrtc(msg: WebRTCMessage) -> Option<WebRTCMessage> {
    if let WebRTCMessage::Output(WebRTCOutput::Setup(m)) = msg {
        log::debug!("Link setup message: {:?}", m);
        Some(WebRTCMessage::Input(WebRTCInput::Setup(m)))
    } else {
        None
    }
}

async fn wait_connected(tag: String, broker: &mut Broker<WebRTCMessage>) -> oneshot::Receiver<bool> {
    let (tap, tap_ind) = broker.get_tap().await.expect("Broker tap");
    let (tx, rx) = oneshot::channel();
    let mut broker_clone = broker.clone();
    tokio::spawn(async move {
        for msg in tap {
            log::trace!("{tag}: Got message: {:?}", msg);
            if let WebRTCMessage::Output(WebRTCOutput::Connected) = msg {
                tx.send(true).unwrap();
                break;
            }
        }
        log::trace!("Done {tag}");
        if let Err(e) = broker_clone.remove_subsystem(tap_ind).await{
            log::error!("Couldn't remove tap: {:?}", e);
        }
        });
    rx
}

#[tokio::test(flavor = "multi_thread")]
async fn connection_setup_forward() -> Result<(), SetupError> {
    flarch::start_logging_filter(vec!["web_rtc_connection", "flnet_libc", "flarch"]);
    // flarch::start_logging();

    let mut init = WebRTCConnectionSetupLibc::new_box().await?;
    let mut follow = WebRTCConnectionSetupLibc::new_box().await?;
    log::debug!("Started init and follow node");

    let init_conn = wait_connected("init  ".into(), &mut init).await;
    let follow_conn = wait_connected("follow".into(), &mut follow).await;

    init.link_bi(follow.clone(), Box::new(link_webrtc), Box::new(link_webrtc)).await;
    log::debug!("Sending init message");
    init.emit_msg(WebRTCMessage::Input(WebRTCInput::Setup(PeerMessage::Init)))
        .await?;

    init_conn.await?;
    log::debug!("Init connected");
    follow_conn.await?;
    log::debug!("Follow connected");

    check_connected(&mut init, &mut follow).await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn connection_reconnect() -> Result<(), SetupError> {
    flarch::start_logging_filter(vec!["web_rtc_connection", "flnet_libc", "flarch"]);
    // flarch::start_logging();

    let mut init = WebRTCConnectionSetupLibc::new_box().await?;
    let mut follow = WebRTCConnectionSetupLibc::new_box().await?;
    init.link_bi(follow.clone(), Box::new(link_webrtc), Box::new(link_webrtc)).await;
    log::debug!("Started init and follow node");

    for i in 1..=10 {
        log::info!("Looping {i}");
        let init_conn = wait_connected("init  ".into(), &mut init).await;
        let follow_conn = wait_connected("follow".into(), &mut follow).await;

        log::debug!("Sending init message");
        init.emit_msg(WebRTCMessage::Input(WebRTCInput::Setup(PeerMessage::Init)))
            .await?;

        init_conn.await?;
        log::debug!("Init connected");
        follow_conn.await?;
        log::debug!("Follow connected");

        check_connected(&mut init, &mut follow).await?;

        log::info!("Resetting init");
        init.emit_msg(WebRTCMessage::Input(WebRTCInput::Reset)).await?;
        wait_ms(1000).await;

        log::info!("Resetting follow");
        follow.emit_msg(WebRTCMessage::Input(WebRTCInput::Reset)).await?;
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn connection_setup_loop() -> Result<(), SetupError> {
    flarch::start_logging_filter(vec!["web_rtc_connection", "flnet_libc", "flarch"]);
    // flarch::start_logging();

    let mut init = WebRTCConnectionSetupLibc::new_box().await?;
    let mut follow = WebRTCConnectionSetupLibc::new_box().await?;
    let (init_tap, _) = init.get_tap().await?;
    let (follow_tap, _) = follow.get_tap().await?;
    log::debug!("Started init and follow node");

    init.emit_msg(WebRTCMessage::Input(WebRTCInput::Setup(PeerMessage::Init)))
        .await?;
    log::debug!("Sent init message");

    for i in 0..20 {
        log::debug!("Looping {i}");
        if let Ok(msg) = follow_tap.try_recv() {
            log::debug!("Follow: got {msg:?}");
            if let WebRTCMessage::Output(WebRTCOutput::Setup(m)) = msg {
                init.emit_msg_dest(
                    Destination::NoTap,
                    WebRTCMessage::Input(WebRTCInput::Setup(m)),
                )
                .await?;
            }
        }
        if let Ok(msg) = init_tap.try_recv() {
            log::debug!("Init  : got {msg:?}");
            if let WebRTCMessage::Output(WebRTCOutput::Setup(m)) = msg {
                follow
                    .emit_msg_dest(
                        Destination::NoTap,
                        WebRTCMessage::Input(WebRTCInput::Setup(m)),
                    )
                    .await?;
            }
        }
        wait_ms(200).await;
    }

    check_connected(&mut init, &mut follow).await?;
    Ok(())
}

async fn check_connected(
    init: &mut Broker<WebRTCMessage>,
    follow: &mut Broker<WebRTCMessage>,
) -> Result<(), SetupError> {
    let (init_tap, init_ind) = init.get_tap().await?;
    let (follow_tap, follow_ind) = follow.get_tap().await?;

    init.emit_msg_dest(
        Destination::NoTap,
        WebRTCMessage::Input(WebRTCInput::Text("init->follow".into())),
    )
    .await?;
    assert_eq!(
        follow_tap.recv()?,
        WebRTCMessage::Output(WebRTCOutput::Text("init->follow".into()))
    );
    log::debug!("Got message from init");

    follow
        .emit_msg_dest(
            Destination::NoTap,
            WebRTCMessage::Input(WebRTCInput::Text("follow->init".into())),
        )
        .await?;
    assert_eq!(
        init_tap.recv()?,
        WebRTCMessage::Output(WebRTCOutput::Text("follow->init".into()))
    );
    log::debug!("Got message from follow");

    init.remove_subsystem(init_ind).await?;
    follow.remove_subsystem(follow_ind).await?;
    Ok(())
}
