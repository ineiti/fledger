use async_trait::async_trait;
use futures::{
    lock::Mutex,
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use std::sync::Arc;
use tokio::{
    net::{TcpListener, TcpStream},
    select,
    sync::oneshot,
    task::JoinHandle,
};
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};

use crate::web_rtc::websocket::{WSError, WSSError, WSServerIn, WSServerOut};
use crate::{
    broker::{Broker, SubsystemHandler},
    web_rtc::websocket::BrokerWSServer,
};

pub struct WebSocketServer {
    connections: Arc<Mutex<Vec<WSConnection>>>,
    conn_thread: JoinHandle<()>,
}

impl WebSocketServer {
    pub async fn new(port: u16) -> Result<BrokerWSServer, WSSError> {
        let server = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
        let connections = Arc::new(Mutex::new(Vec::new()));
        let connections_cl = Arc::clone(&connections);
        let mut broker = Broker::new();
        let mut broker_cl = broker.clone();
        let conn_thread = tokio::spawn(async move {
            let mut connection_id = 0;
            loop {
                if let Ok((stream, _)) = server.accept().await {
                    let broker_cl2 = broker_cl.clone();
                    match WSConnection::new(stream, broker_cl2, connection_id).await {
                        Ok(conn) => {
                            log::trace!("Got new connection");
                            connections_cl.lock().await.push(conn);
                            broker_cl
                                .emit_msg_out(WSServerOut::NewConnection(connection_id))
                                .expect("Error sending connect message");
                        }
                        Err(e) => log::error!("Error while getting connection: {:?}", e),
                    }
                    connection_id += 1;
                }
            }
        });

        broker
            .add_handler(Box::new(WebSocketServer {
                connections,
                conn_thread,
            }))
            .await?;

        Ok(broker)
    }
}

#[async_trait]
impl SubsystemHandler<WSServerIn, WSServerOut> for WebSocketServer {
    async fn messages(&mut self, from_broker: Vec<WSServerIn>) -> Vec<WSServerOut> {
        for msg in from_broker {
            match msg {
                WSServerIn::Message(id, msg) => {
                    let mut connections = self.connections.lock().await;
                    if let Some(conn) = connections.get_mut(id) {
                        if let Err(e) = conn.send(msg).await {
                            log::error!("Error while sending: {e}");
                            conn.close();
                            connections.remove(id);
                        }
                    }
                }
                WSServerIn::Close(id) => {
                    let mut connections = self.connections.lock().await;
                    if let Some(conn) = connections.get_mut(id) {
                        conn.close();
                        connections.remove(id);
                    }
                }
                WSServerIn::Stop => {
                    log::trace!("Stopping thread");
                    self.conn_thread.abort();
                    return vec![WSServerOut::Stopped];
                }
            }
        }
        vec![]
    }
}

pub struct WSConnection {
    websocket: SplitSink<WebSocketStream<TcpStream>, Message>,
    tx: Option<oneshot::Sender<bool>>,
}

impl WSConnection {
    async fn new(
        stream: TcpStream,
        broker: BrokerWSServer,
        id: usize,
    ) -> Result<WSConnection, WSError> {
        let websocket = accept_async(stream)
            .await
            .map_err(|e| WSError::Underlying(e.to_string()))?;
        let (wr, rd) = websocket.split();
        let (tx, rx) = oneshot::channel();
        let uwsc = WSConnection {
            websocket: wr,
            tx: Some(tx),
        };

        WSConnection::loop_read(broker, rd, rx, id).await;
        Ok(uwsc)
    }

    async fn loop_read(
        mut broker: BrokerWSServer,
        mut ws: SplitStream<WebSocketStream<TcpStream>>,
        mut rx: oneshot::Receiver<bool>,
        id: usize,
    ) {
        tokio::spawn(async move {
            loop {
                select! {
                    _ = (&mut rx) => {
                        broker
                        .emit_msg_out(WSServerOut::Disconnection(id))
                        .expect("While sending message to broker.");
                        return;
                    },
                    ws_out = ws.next() =>
                        if let Some(msg_ws) = ws_out {
                            if let Some(out) = match msg_ws {
                                Ok(msg) => match msg {
                                    Message::Text(s) => {
                                        Some(WSServerOut::Message(id, s))
                                    }
                                    Message::Close(_) => {
                                        Some(WSServerOut::Disconnection(id))
                                    }
                                    _ => None,
                                },
                                Err(e) => {
                                    log::warn!("Closing connection because of error: {e:?}");
                                    Some(WSServerOut::Disconnection(id))
                                }
                            } {
                                broker
                                    .emit_msg_out(out.clone())
                                    .expect("While sending message to broker.");
                                if matches!(
                                    out,
                                    WSServerOut::Disconnection(_)
                                ) {
                                    return;
                                }
                            }
                        }
                }
            }
        });
    }

    async fn send(&mut self, msg: String) -> Result<(), WSError> {
        self.websocket
            .send(Message::Text(msg))
            .await
            .map_err(|e| WSError::Underlying(e.to_string()))
    }

    fn close(&mut self) {
        if let Some(tx) = self.tx.take() {
            tx.send(true)
                .err()
                .map(|_| log::error!("Closing Websocket failed"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::Destination;
    use crate::start_logging_filter_level;
    use crate::tasks::wait_ms;
    use crate::web_rtc::web_socket_client::WebSocketClient;
    use crate::web_rtc::websocket::{BrokerWSClient, WSClientIn, WSClientOut};
    use std::error::Error;
    use std::sync::mpsc::Receiver;

    async fn send_client_server(
        client: &mut BrokerWSClient,
        server_tap: &Receiver<WSServerOut>,
        ch_index: usize,
        txt: String,
    ) {
        client
            .settle_msg_in_dest(Destination::NoTap, WSClientIn::Message(txt.clone()))
            .await
            .unwrap();
        assert_eq!(
            server_tap.recv().unwrap(),
            WSServerOut::Message(ch_index, txt)
        );
    }

    async fn send_server_client(
        server: &mut BrokerWSServer,
        client_tap: &Receiver<WSClientOut>,
        ch_index: usize,
        txt: String,
    ) {
        server
            .settle_msg_in_dest(
                Destination::NoTap,
                WSServerIn::Message(ch_index, txt.clone()),
            )
            .await
            .unwrap();
        assert_eq!(client_tap.recv().unwrap(), WSClientOut::Message(txt));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_server() -> Result<(), Box<dyn Error>> {
        start_logging_filter_level(vec![], log::LevelFilter::Info);
        let mut server = WebSocketServer::new(8080).await.unwrap();
        let (server_tap, tap0) = server.get_tap_out_sync().await.unwrap();

        wait_ms(100).await;

        let mut client1 = WebSocketClient::connect("ws://localhost:8080")
            .await
            .unwrap();
        let (client1_tap, tap1) = client1.get_tap_out_sync().await.unwrap();
        let reply = server_tap.recv();
        log::debug!("Server reply from client 1: {:?}", reply);

        wait_ms(100).await;

        let mut client2 = WebSocketClient::connect("ws://localhost:8080")
            .await
            .unwrap();
        let (client2_tap, _) = client2.get_tap_out_sync().await.unwrap();
        let reply = server_tap.recv();
        log::debug!("Server reply from client 2: {:?}", reply);

        wait_ms(100).await;

        for _ in 1..=2 {
            send_client_server(&mut client1, &server_tap, 0, "Hello 1".to_string()).await;
            send_server_client(&mut server, &client1_tap, 0, "there 1".to_string()).await;

            send_client_server(&mut client2, &server_tap, 1, "Hello 2".to_string()).await;
            send_server_client(&mut server, &client2_tap, 1, "there 2".to_string()).await;
        }

        client1.emit_msg_in(WSClientIn::Disconnect).unwrap();
        client2.emit_msg_in(WSClientIn::Disconnect).unwrap();
        server.emit_msg_in(WSServerIn::Stop).unwrap();

        server.remove_subsystem(tap0).await?;
        client1.remove_subsystem(tap1).await?;

        Ok(())
    }
}
