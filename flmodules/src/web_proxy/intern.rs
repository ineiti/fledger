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
use crate::router::messages::{RouterIn, RouterOut};
use crate::web_proxy::broker::{WebProxyIn, WebProxyOut};
use crate::Modules;

use super::broker::MODULE_NAME;
use super::{core::*, response::ResponseMessage};

/// Messages between different instances of this module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum ModuleMessage {
    /// The request holds a random ID so that the reply can be mapped to the correct
    /// request.
    Request(U256, String),
    /// The reply uses the random ID from the request and returns blocks of the
    /// reply, indexed by the second argument.
    Response(U256, ResponseMessage),
}

#[derive(Debug, Clone)]
pub(super) enum InternIn {
    Router(RouterOut),
    WebProxy(WebProxyIn),
}

#[derive(Debug, Clone)]
pub(super) enum InternOut {
    Router(RouterIn),
    WebProxy(WebProxyOut),
}

pub(super) type BrokerIntern = Broker<InternIn, InternOut>;

/// The message handling part, but only for WebProxy messages.
pub(super) struct Intern {
    core: WebProxyCore,
    // Needs a copy of the broker to use it in spwan_local while waiting for
    // http-get result.
    broker: BrokerIntern,
    tx: Option<watch::Sender<WebProxyStorage>>,
}

impl Intern {
    /// Returns a new chat module.
    pub async fn new(
        ds: Box<dyn DataStorage + Send>,
        cfg: WebProxyConfig,
        our_id: NodeID,
    ) -> anyhow::Result<(BrokerIntern, watch::Receiver<WebProxyStorage>)> {
        let str = ds.get(MODULE_NAME).unwrap_or("".into());
        let storage = WebProxyStorageSave::from_str(&str).unwrap_or_default();
        let (tx, rx) = watch::channel(storage.clone());
        let mut intern = Broker::new();
        intern
            .add_handler(Box::new(Self {
                core: WebProxyCore::new(storage, cfg, our_id),
                broker: intern.clone(),
                tx: Some(tx),
            }))
            .await?;

        Ok((intern, rx))
    }

    fn msg_router(&mut self, msg: RouterOut) -> Vec<InternOut> {
        match msg {
            RouterOut::NodeInfosConnected(node_infos) => self.node_list(node_infos),
            RouterOut::NetworkWrapperFromNetwork(u256, network_wrapper) => network_wrapper
                .unwrap_yaml(MODULE_NAME)
                .map(|msg| self.process_node_message(u256, msg))
                .unwrap_or(vec![]),
            _ => vec![],
        }
    }

    fn msg_proxy(&mut self, msg: WebProxyIn) -> Vec<InternOut> {
        match msg {
            WebProxyIn::RequestGet(u256, url, sender) => self.request_get(u256, url, sender),
        }
    }

    // Processes a node to node message and returns zero or more
    // MessageOut.
    fn process_node_message(&mut self, src: NodeID, msg: ModuleMessage) -> Vec<InternOut> {
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
    fn node_list(&mut self, nodes: Vec<NodeInfo>) -> Vec<InternOut> {
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

    fn request_get(&mut self, rnd: U256, url: String, tx: Sender<Bytes>) -> Vec<InternOut> {
        self.core.request_get(rnd, tx).map_or(vec![], |node| {
            vec![Self::module_msg(node, &ModuleMessage::Request(rnd, url))]
        })
    }

    fn module_msg(node: NodeID, msg: &ModuleMessage) -> InternOut {
        InternOut::Router(RouterIn::NetworkWrapperToNetwork(
            node,
            crate::router::messages::NetworkWrapper::wrap_yaml(MODULE_NAME, &msg).unwrap(),
        ))
    }

    fn start_request(&mut self, src: NodeID, nonce: U256, request: String) -> Vec<InternOut> {
        let mut broker = self.broker.clone();
        spawn_local(async move {
            match reqwest::get(request).await {
                Ok(resp) => {
                    broker
                        .emit_msg_out(Self::module_msg(
                            src,
                            &ModuleMessage::Response(
                                nonce,
                                ResponseMessage::Header((&resp).into()),
                            ),
                        ))
                        .expect("sending header");
                    let mut stream = resp.bytes_stream().chunks(1024);
                    while let Some(chunks) = stream.next().await {
                        for chunk in chunks {
                            broker
                                .emit_msg_out(Self::module_msg(
                                    src,
                                    &ModuleMessage::Response(
                                        nonce,
                                        ResponseMessage::Body(chunk.expect("getting chunk")),
                                    ),
                                ))
                                .expect("sending body");
                        }
                    }
                    broker
                        .emit_msg_out(Self::module_msg(
                            src,
                            &ModuleMessage::Response(nonce, ResponseMessage::Done),
                        ))
                        .expect("sending done");
                }
                Err(e) => {
                    broker
                        .emit_msg_out(Self::module_msg(
                            src,
                            &ModuleMessage::Response(nonce, ResponseMessage::Error(e.to_string())),
                        ))
                        .expect("Sending done message for");
                    return;
                }
            }
            broker
                .emit_msg_out(Self::module_msg(
                    src,
                    &ModuleMessage::Response(nonce, ResponseMessage::Done),
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
    ) -> Vec<InternOut> {
        self.core
            .handle_response(nonce, msg)
            .map_or(vec![], |header| {
                vec![InternOut::WebProxy(WebProxyOut::ResponseGet(
                    src, nonce, header,
                ))]
            })
    }
}

#[platform_async_trait()]
impl SubsystemHandler<InternIn, InternOut> for Intern {
    async fn messages(&mut self, msgs: Vec<InternIn>) -> Vec<InternOut> {
        msgs.into_iter()
            .flat_map(|msg| match msg {
                InternIn::Router(router_out) => self.msg_router(router_out),
                InternIn::WebProxy(web_proxy_in) => self.msg_proxy(web_proxy_in),
            })
            .collect::<Vec<_>>()
    }
}
