use async_trait::async_trait;
use futures::stream::SplitStream;
use futures::{stream::SplitSink, Sink, SinkExt, StreamExt};
use std::pin::Pin;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite, MaybeTlsStream, WebSocketStream};

use crate::broker::{Broker, SubsystemHandler};
use crate::tasks::wait_ms;
use crate::web_rtc::websocket::{BrokerWSClient, WSClientIn, WSClientOut, WSError};

pub struct WebSocketClient {
    url: String,
    write: Option<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>>,
    broker: BrokerWSClient,
}

impl WebSocketClient {
    pub async fn connect(url: &str) -> anyhow::Result<BrokerWSClient> {
        let wsb = WebSocketClient {
            url: url.to_string(),
            write: None,
            broker: Broker::new(),
        };
        let mut broker = wsb.broker.clone();
        broker.add_handler(Box::new(wsb)).await?;
        broker.emit_msg_in(WSClientIn::Connect)?;
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
                                    .emit_msg_out(WSClientOut::Message(msg.to_string()))
                                    .expect("Failed to emit message");
                            }
                        }
                        Err(e) => {
                            log::warn!("Closing connection because of error: {:?}", e);
                            log::warn!("Trying to reconnect");
                            broker_cl
                                .emit_msg_in(WSClientIn::Connect)
                                .expect("tried to reconnect");
                            return;
                        }
                    }
                }
            }
        });
    }

    async fn connect_ws(&mut self) -> anyhow::Result<()> {
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
impl SubsystemHandler<WSClientIn, WSClientOut> for WebSocketClient {
    async fn messages(&mut self, msgs: Vec<WSClientIn>) -> Vec<WSClientOut> {
        for msg in msgs {
            match msg {
                WSClientIn::Message(msg) => {
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
                WSClientIn::Disconnect => {
                    if let Some(mut write) = self.write.take() {
                        write.close().await.unwrap();
                    } else {
                        log::warn!("Trying to disconnect a disconnected connection");
                    }
                    return vec![WSClientOut::Disconnect];
                }
                WSClientIn::Connect => {
                    if let Err(e) = self.connect_ws().await {
                        log::error!("Couldn't connect to {}: {e}", self.url);
                    }
                }
            }
        }
        vec![]
    }
}
