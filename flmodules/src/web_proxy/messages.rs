use bytes::Bytes;
use flarch::broker::{Broker, SubsystemHandler};
use flarch::data_storage::DataStorage;
use flarch::nodeids::{NodeID, U256};
use flarch::platform_async_trait;
use flarch::tasks::spawn_local;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;
use tokio::sync::watch;

use crate::nodeconfig::NodeInfo;
use crate::Modules;

use super::broker::MODULE_NAME;
use super::{
    core::*,
    response::{ResponseHeader, ResponseMessage},
};

/// Messages between different instances of this module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModuleMessage {
    /// The request holds a random ID so that the reply can be mapped to the correct
    /// request.
    Request(U256, String),
    /// The reply uses the random ID from the request and returns blocks of the
    /// reply, indexed by the second argument.
    Response(U256, ResponseMessage),
}

/// All possible calls TO this module.
#[derive(Debug, Clone)]
pub enum WebProxyIn {
    FromNetwork(NodeID, ModuleMessage),
    NodeInfoConnected(Vec<NodeInfo>),
    RequestGet(U256, String, Sender<Bytes>),
}

/// All possible replies FROM this module.
#[derive(Debug, Clone)]
pub enum WebProxyOut {
    ToNetwork(NodeID, ModuleMessage),
    ResponseGet(NodeID, U256, ResponseHeader),
}

/// The message handling part, but only for WebProxy messages.
pub struct Messages {
    core: WebProxyCore,
    broker: Broker<WebProxyIn, WebProxyOut>,
    tx: Option<watch::Sender<WebProxyStorage>>,
}

impl Messages {
    /// Returns a new simulation_chat module.
    pub fn new(
        ds: Box<dyn DataStorage + Send>,
        cfg: WebProxyConfig,
        our_id: NodeID,
        broker: Broker<WebProxyIn, WebProxyOut>,
    ) -> (Self, watch::Receiver<WebProxyStorage>) {
        let str = ds.get(MODULE_NAME).unwrap_or("".into());
        let storage = WebProxyStorageSave::from_str(&str).unwrap_or_default();

        let (tx, rx) = watch::channel(storage.clone());
        (
            Self {
                core: WebProxyCore::new(storage, cfg, our_id),
                broker,
                tx: Some(tx),
            },
            rx,
        )
    }

    /// Processes one generic message and returns either an error
    /// or a Vec<MessageOut>.
    pub fn process_messages(&mut self, msgs: Vec<WebProxyIn>) -> Vec<WebProxyOut> {
        msgs.into_iter()
            .map(|msg| match msg {
                WebProxyIn::FromNetwork(src, node_msg) => self.process_node_message(src, node_msg),
                WebProxyIn::NodeInfoConnected(ids) => self.node_list(ids),
                WebProxyIn::RequestGet(rnd, url, tx) => self.request_get(rnd, url, tx),
            })
            .flatten()
            .collect()
    }

    /// Processes a node to node message and returns zero or more
    /// MessageOut.
    pub fn process_node_message(&mut self, src: NodeID, msg: ModuleMessage) -> Vec<WebProxyOut> {
        let out = match msg {
            ModuleMessage::Request(nonce, request) => self.start_request(src, nonce, request),
            ModuleMessage::Response(nonce, response) => self.handle_response(src, nonce, response),
        };
        self.tx.clone().map(|tx| {
            tx.send(self.core.storage.clone())
                .is_err()
                .then(|| self.tx = None)
        });
        out
    }

    /// Stores the new node list, excluding the ID of this node.
    fn node_list(&mut self, nodes: Vec<NodeInfo>) -> Vec<WebProxyOut> {
        self.core.node_list(
            nodes
                .iter()
                .filter(|ni| ni.modules.contains(Modules::WEBPROXY_REQUESTS))
                .map(|ni| ni.get_id())
                .collect::<Vec<NodeID>>()
                .into(),
        );
        vec![]
    }

    fn request_get(&mut self, rnd: U256, url: String, tx: Sender<Bytes>) -> Vec<WebProxyOut> {
        self.core.request_get(rnd, tx).map_or(vec![], |node| {
            vec![WebProxyOut::ToNetwork(
                node,
                ModuleMessage::Request(rnd, url),
            )]
        })
    }

    fn start_request(&mut self, src: NodeID, nonce: U256, request: String) -> Vec<WebProxyOut> {
        let mut broker = self.broker.clone();
        spawn_local(async move {
            match reqwest::get(request).await {
                Ok(resp) => {
                    broker
                        .emit_msg_out(WebProxyOut::ToNetwork(
                            src,
                            ModuleMessage::Response(nonce, ResponseMessage::Header((&resp).into())),
                        ))
                        .expect("sending header");
                    let mut stream = resp.bytes_stream().chunks(1024);
                    while let Some(chunks) = stream.next().await {
                        for chunk in chunks {
                            broker
                                .emit_msg_out(WebProxyOut::ToNetwork(
                                    src,
                                    ModuleMessage::Response(
                                        nonce,
                                        ResponseMessage::Body(chunk.expect("getting chunk")),
                                    ),
                                ))
                                .expect("sending body");
                        }
                    }
                    broker
                        .emit_msg_out(WebProxyOut::ToNetwork(
                            src,
                            ModuleMessage::Response(nonce, ResponseMessage::Done),
                        ))
                        .expect("sending done");
                }
                Err(e) => {
                    broker
                        .emit_msg_out(WebProxyOut::ToNetwork(
                            src,
                            ModuleMessage::Response(nonce, ResponseMessage::Error(e.to_string())),
                        ))
                        .expect("Sending done message for");
                    return;
                }
            }
            broker
                .emit_msg_out(WebProxyOut::ToNetwork(
                    src,
                    ModuleMessage::Response(nonce, ResponseMessage::Done),
                ))
                .expect("Sending done message for");
        });
        vec![]
    }

    fn handle_response(
        &mut self,
        src: NodeID,
        nonce: U256,
        msg: ResponseMessage,
    ) -> Vec<WebProxyOut> {
        self.core
            .handle_response(nonce, msg)
            .map_or(vec![], |header| {
                vec![WebProxyOut::ResponseGet(src, nonce, header)]
            })
    }
}

#[platform_async_trait()]
impl SubsystemHandler<WebProxyIn, WebProxyOut> for Messages {
    async fn messages(&mut self, msgs: Vec<WebProxyIn>) -> Vec<WebProxyOut> {
        self.process_messages(msgs)
    }
}
