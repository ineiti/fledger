use flmodules::broker::{Broker, BrokerError, Translate};

use super::websocket::{
    WSClientInput, WSClientMessage, WSClientOutput, WSServerInput, WSServerMessage, WSServerOutput,
};

pub struct WebSocketSimul {
    pub server: Broker<WSServerMessage>,
    clients: Vec<Broker<WSClientMessage>>,
}

impl WebSocketSimul {
    pub fn new() -> Self {
        Self {
            server: Broker::new(),
            clients: vec![],
        }
    }

    pub async fn new_connection(
        &mut self,
    ) -> Result<(usize, Broker<WSClientMessage>), BrokerError> {
        let broker = Broker::new();
        self.clients.push(broker.clone());
        let index = self.clients.len() - 1;
        self.server
            .link_bi(
                broker.clone(),
                Self::cl_to_srv(index),
                Self::srv_to_cl(index),
            )
            .await;
        Ok((index, broker))
    }

    pub async fn announce_connection(&mut self, index: usize) -> Result<(), BrokerError> {
        self.server
            .emit_msg(WSServerOutput::Connect(index).into())
            .await?;
        Ok(())
    }

    pub async fn close_connection(&mut self, index: usize) -> Result<(), BrokerError> {
        self.clients.remove(index);
        self.server
            .emit_msg(WSServerOutput::Disconnect(index).into())
            .await?;
        Ok(())
    }

    fn cl_to_srv(index: usize) -> Translate<WSClientMessage, WSServerMessage> {
        Box::new(move |msg: WSClientMessage| {
            if let WSClientMessage::Input(WSClientInput::Message(msg)) = msg {
                return Some(WSServerMessage::Output(WSServerOutput::Message((
                    index, msg,
                ))));
            }
            None
        })
    }

    fn srv_to_cl(index: usize) -> Translate<WSServerMessage, WSClientMessage> {
        Box::new(move |msg: WSServerMessage| {
            if let WSServerMessage::Input(WSServerInput::Message((index_dst, msg_str))) = msg {
                return (index == index_dst).then(|| WSClientOutput::Message(msg_str).into());
            }
            None
        })
    }
}
