use async_trait::async_trait;
use flarch::tasks::wait_ms;
use flmodules::broker::{Broker, Destination, Subsystem, SubsystemListener};
use tokio::net::TcpStream;

use futures::{stream::SplitSink, Sink, SinkExt, StreamExt};
use std::pin::Pin;
use tokio_tungstenite::{connect_async, tungstenite, MaybeTlsStream, WebSocketStream};

use crate::websocket::{WSClientInput, WSClientMessage, WSClientOutput, WSError};

use crate::websocket::WSSError;

pub struct WebSocketClient {
    write: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>,
}

impl WebSocketClient {
    pub async fn connect(url: &str) -> Result<Broker<WSClientMessage>, WSSError> {
        log::debug!("Connecting to websocket at {}", url);
        let (websocket, _) = connect_async(url).await?;

        let (write, mut read) = websocket.split();
        let wsc = WebSocketClient { write };
        let mut broker = Broker::new();
        broker
            .add_subsystem(Subsystem::Handler(Box::new(wsc)))
            .await?;
        let mut broker_cl = broker.clone();
        tokio::spawn(async move {
            wait_ms(1000).await;
            loop {
                for msg in read.next().await {
                    match msg {
                        Ok(msg) => {
                            if msg.is_text() {
                                broker_cl
                                    .emit_msg(WSClientOutput::Message(msg.to_string()).into())
                                    .await
                                    .expect("Failed to emit message");
                            }
                        }
                        Err(e) => {
                            log::warn!("Closing connection: {:?}", e);
                            return;
                        }
                    }
                }
            }
        });
        Ok(broker)
    }
}

#[async_trait]
impl SubsystemListener<WSClientMessage> for WebSocketClient {
    async fn messages(
        &mut self,
        msgs: Vec<WSClientMessage>,
    ) -> Vec<(Destination, WSClientMessage)> {
        for msg in msgs {
            if let WSClientMessage::Input(msg_in) = msg {
                match msg_in {
                    WSClientInput::Message(msg) => {
                        Pin::new(&mut self.write)
                            .start_send(tungstenite::Message::text(msg))
                            .map_err(|e| WSError::Underlying(e.to_string()))
                            .expect("Error sending message");
                        Pin::new(&mut self.write)
                            .flush()
                            .await
                            .map_err(|e| WSError::Underlying(e.to_string()))
                            .expect("msg flush error");
                    }
                    WSClientInput::Disconnect => {
                        self.write.close().await.unwrap();
                        return vec![(Destination::Others, WSClientOutput::Disconnect.into())];
                    }
                }
            }
        }
        vec![]
    }
}
