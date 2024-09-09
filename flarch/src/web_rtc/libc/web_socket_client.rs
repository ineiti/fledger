use async_trait::async_trait;
use futures::stream::SplitStream;
use futures::{stream::SplitSink, Sink, SinkExt, StreamExt};
use std::pin::Pin;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite, MaybeTlsStream, WebSocketStream};

use crate::broker::{Broker, Subsystem, SubsystemHandler};
use crate::tasks::wait_ms;
use crate::web_rtc::websocket::{
    WSClientInput, WSClientMessage, WSClientOutput, WSError, WSSError,
};

pub struct WebSocketClient {
    url: String,
    write: Option<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>>,
    broker: Broker<WSClientMessage>,
}

impl WebSocketClient {
    pub async fn connect(url: &str) -> Result<Broker<WSClientMessage>, WSSError> {
        let wsb = WebSocketClient {
            url: url.to_string(),
            write: None,
            broker: Broker::new(),
        };
        let mut broker = wsb.broker.clone();
        broker
            .add_subsystem(Subsystem::Handler(Box::new(wsb)))
            .await?;
        broker.emit_msg(WSClientMessage::Input(WSClientInput::Connect))?;
        Ok(broker)
    }

    fn listen(&mut self, mut read: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>) {
        let mut broker_cl = self.broker.clone();
        tokio::spawn(async move {
            wait_ms(1000).await;
            loop {
                if let Some(msg) = read.next().await {
                    match msg {
                        Ok(msg) => {
                            if msg.is_text() {
                                broker_cl
                                    .emit_msg(WSClientOutput::Message(msg.to_string()).into())
                                    .expect("Failed to emit message");
                            }
                        }
                        Err(e) => {
                            log::warn!("Closing connection because of error: {:?}", e);
                            log::warn!("Trying to reconnect");
                            broker_cl
                                .emit_msg(WSClientMessage::Input(WSClientInput::Connect))
                                .expect("tried to reconnect");
                            return;
                        }
                    }
                }
            }
        });
    }

    async fn connect_ws(&mut self) -> Result<(), WSSError> {
        if let Some(mut write) = self.write.take() {
            log::debug!("Reconnecting to websocket at {}", self.url);
            write.close().await?;
        } else {
            log::debug!("Connecting to websocket at {}", self.url);
        }
        let (websocket, _) = connect_async(self.url.clone()).await?;
        let (write, read) = websocket.split();
        self.write = Some(write);
        self.listen(read);

        Ok(())
    }
}

#[async_trait]
impl SubsystemHandler<WSClientMessage> for WebSocketClient {
    async fn messages(&mut self, msgs: Vec<WSClientMessage>) -> Vec<WSClientMessage> {
        for msg in msgs {
            if let WSClientMessage::Input(msg_in) = msg {
                match msg_in {
                    WSClientInput::Message(msg) => {
                        if let Some(mut write) = self.write.as_mut() {
                            Pin::new(&mut write)
                                .start_send(tungstenite::Message::text(msg))
                                .map_err(|e| WSError::Underlying(e.to_string()))
                                .expect("Error sending message");
                            Pin::new(&mut write)
                                .flush()
                                .await
                                .map_err(|e| WSError::Underlying(e.to_string()))
                                .expect("msg flush error");
                        } else {
                            log::warn!("Tried to write a message to a closed connection");
                            if let Err(e) = self.connect_ws().await {
                                log::error!("Couldn't connect: {e}");
                            }
                        }
                    }
                    WSClientInput::Disconnect => {
                        if let Some(mut write) = self.write.take() {
                            write.close().await.unwrap();
                        } else {
                            log::warn!("Trying to disconnect a disconnected connection");
                        }
                        return vec![WSClientOutput::Disconnect.into()];
                    }
                    WSClientInput::Connect => {
                        if let Err(e) = self.connect_ws().await {
                            log::error!("Couldn't connect: {e}");
                        }
                    }
                }
            }
        }
        vec![]
    }
}
