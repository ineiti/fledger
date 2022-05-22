use flnet::websocket::{WSServerMessage, WSServerOutput, 
    WSClientInput, WSClientOutput, WSClientMessage, WSServerInput};
use flnet_libc::{
    web_socket_client::WebSocketClient,
    web_socket_server,
};
use flarch::start_logging;
use thiserror::Error;

#[derive(Error, Debug)]
enum ConnectError {
    #[error(transparent)]
    WS(#[from] flnet::websocket::WSError),
    #[error(transparent)]
    WSS(#[from] web_socket_server::WSSError),
    #[error(transparent)]
    Recv(#[from] std::sync::mpsc::RecvError),
    #[error(transparent)]
    Broker(#[from] flmodules::broker::BrokerError),
}

#[tokio::test(flavor = "multi_thread")]
async fn connect_websocket() -> Result<(), ConnectError> {
    start_logging();

    let mut server = web_socket_server::WebSocketServer::new(8765).await?;
    let (server_tap, _) = server.get_tap().await?;
    let mut ws = WebSocketClient::connect("ws://localhost:8765").await?;

    let msg = server_tap.recv()?;
    let id = if let WSServerMessage::Output(WSServerOutput::Connect(id)) = msg {
        id
    } else {
        panic!("Didn't get a connection");
    };

    log::debug!("Sending ping");
    ws.emit_msg(WSClientInput::Message("ping".to_string()).into())
        .await?;
    let msg = server_tap.recv()?;
    if let WSServerMessage::Output(WSServerOutput::Message((idp, msg))) = msg {
        assert_eq!(id, idp);
        assert_eq!(msg, "ping");
    } else {
        panic!("Unexpected message: {:?}", msg);
    }

    log::debug!("Sending pong");
    let (ws_tap, _) = ws.get_tap().await?;
    server
        .emit_msg(WSServerMessage::Input(WSServerInput::Message((
            id,
            "pong".to_string(),
        ))))
        .await?;
    // Flus the pong-message from the server_tap
    server_tap.recv()?;
    let ws_msg = ws_tap.recv()?;
    if let WSClientMessage::Output(WSClientOutput::Message(msg)) = ws_msg {
        assert_eq!(msg, "pong");
    } else {
        panic!("Got wrong message: {:?}", ws_msg);
    }

    log::debug!("Sending disconnect");
    ws.emit_msg(WSClientInput::Disconnect.into()).await?;
    // Read the WSCMessage::Input
    ws_tap.recv()?;
    assert_eq!(ws_tap.recv()?, WSClientOutput::Disconnect.into());
    assert_eq!(
        server_tap.recv()?,
        WSServerMessage::Output(WSServerOutput::Disconnect(id))
    );

    log::debug!("Stopping server");
    server.emit_msg(WSServerMessage::Input(WSServerInput::Stop)).await?;
    server_tap.recv()?;
    assert_eq!(server_tap.recv()?, WSServerMessage::Output(WSServerOutput::Stopped));

    Ok(())
}
