use bytes::Bytes;
use flarch::tasks::spawn_local;
use flarch::{
    broker::Broker,
    nodeids::{NodeID, U256},
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use crate::loopix::PROXY_REQUEST_RECEIVED;
use crate::nodeconfig::NodeInfo;
use crate::Modules;

use super::{
    broker::WebProxyError,
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

/// First wrap all messages coming into this module and all messages going out in
/// a single message.
#[derive(Clone, Debug)]
pub enum WebProxyMessage {
    Input(WebProxyIn),
    Output(WebProxyOut),
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
    UpdateStorage(WebProxyStorage),
}

/// The message handling part, but only for WebProxy messages.
pub struct WebProxyMessages {
    pub core: WebProxyCore,
    broker: Broker<WebProxyMessage>,
}

impl WebProxyMessages {
    /// Returns a new chat module.
    pub fn new(
        storage: WebProxyStorage,
        cfg: WebProxyConfig,
        our_id: NodeID,
        broker: Broker<WebProxyMessage>,
    ) -> Result<Self, WebProxyError> {
        Ok(Self {
            core: WebProxyCore::new(storage, cfg, our_id),
            broker,
        })
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
        let mut out = match msg {
            ModuleMessage::Request(nonce, request) => {
                if self.core.has_responded(nonce) {
                    log::debug!("Already responded to nonce {}", nonce);
                    return vec![];
                } else {
                    let res = self.start_request(src, nonce, request);
                    self.core.mark_as_responded(nonce);
                    res
                }
            }
            ModuleMessage::Response(nonce, response) => self.handle_response(src, nonce, response),
        };
        out.push(WebProxyOut::UpdateStorage(self.core.storage.clone()));
        out
    }

    /// Stores the new node list, excluding the ID of this node.
    fn node_list(&mut self, nodes: Vec<NodeInfo>) -> Vec<WebProxyOut> {
        self.core.node_list(
            nodes
                .iter()
                .filter(|ni| ni.modules.contains(Modules::ENABLE_WEBPROXY_REQUESTS))
                .map(|ni| ni.get_id())
                .collect::<Vec<NodeID>>()
                .into(),
        );
        vec![]
    }

    fn request_get(&mut self, rnd: U256, url: String, tx: Sender<Bytes>) -> Vec<WebProxyOut> {
        self.core.request_get(rnd, tx).map_or(vec![], |node| {
            vec![WebProxyOut::ToNetwork(node, ModuleMessage::Request(rnd, url))]
        })
    }

    fn start_request(&mut self, src: NodeID, nonce: U256, request: String) -> Vec<WebProxyOut> {
        PROXY_REQUEST_RECEIVED.inc();
        let mut broker = self.broker.clone();
        spawn_local(async move {
            match reqwest::get(request).await {
                Ok(resp) => {
                    broker
                        .emit_msg(WebProxyMessage::Output(WebProxyOut::ToNetwork(
                            src,
                            ModuleMessage::Response(nonce, ResponseMessage::Header((&resp).into())),
                        )))
                        .expect("sending header");
                    let mut stream = resp.bytes_stream().chunks(1024);
                    while let Some(chunks) = stream.next().await {
                        for chunk in chunks {
                            broker
                                .emit_msg(WebProxyMessage::Output(WebProxyOut::ToNetwork(
                                    src,
                                    ModuleMessage::Response(
                                        nonce,
                                        ResponseMessage::Body(chunk.expect("getting chunk")),
                                    ),
                                )))
                                .expect("sending body");
                        }
                    }
                    broker
                        .emit_msg(WebProxyMessage::Output(WebProxyOut::ToNetwork(
                            src,
                            ModuleMessage::Response(nonce, ResponseMessage::Done),
                        )))
                        .expect("sending done");
                }
                Err(e) => {
                    broker
                        .emit_msg(WebProxyMessage::Output(WebProxyOut::ToNetwork(
                            src,
                            ModuleMessage::Response(nonce, ResponseMessage::Error(e.to_string())),
                        )))
                        .expect("Sending done message for");
                    return;
                }
            }
            broker
                .emit_msg(WebProxyMessage::Output(WebProxyOut::ToNetwork(
                    src,
                    ModuleMessage::Response(nonce, ResponseMessage::Done),
                )))
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

/// Convenience method to reduce long lines.
impl From<WebProxyIn> for WebProxyMessage {
    fn from(msg: WebProxyIn) -> Self {
        WebProxyMessage::Input(msg)
    }
}

/// Convenience method to reduce long lines.
impl From<WebProxyOut> for WebProxyMessage {
    fn from(msg: WebProxyOut) -> Self {
        WebProxyMessage::Output(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() -> Result<(), WebProxyError> {
        Ok(())
    }
}