use std::collections::HashMap;

use flarch::{
    broker::SubsystemHandler,
    data_storage::DataStorage,
    nodeids::{NodeID, NodeIDs},
    platform_async_trait,
    web_rtc::{
        websocket::{WSClientIn, WSClientOut},
        WebRTCConnInput, WebRTCConnOutput,
    },
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::{
    network::broker::{NetworkIn, NetworkOut},
    router::messages::{NetworkWrapper, RouterIn, RouterOut},
};

use super::{broker::MODULE_NAME, core::*};

/// These are the messages which will be exchanged between the nodes for this
/// module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(super) enum ModuleMessage {
    // What other IDs are available at this node?
    NeighborRequest,
    // All connected IDs.
    NeighborIDs(NodeIDs),
    // Send a "WebSocket" message to the other node, which will act
    // as the "signalling server".
    WSMessage(String),
    // An actual message from a node.
    NetworkWrapper(NetworkWrapper),
}

/// The messages here represent all possible interactions with this module.
#[derive(Debug, Clone)]
pub(super) enum InternIn {
    Router(RouterIn),
    NetworkSignal(NetworkOut),
    NetworkNode(NetworkOut),
    WebSocket(WSClientOut),
    WebRTC(WebRTCConnOutput),
}

#[derive(Debug, Clone)]
pub(super) enum InternOut {
    Router(RouterOut),
    NetworkSignal(NetworkIn),
    NetworkNode(NetworkIn),
    WebSocket(WSClientIn),
    WebRTC(WebRTCConnInput),
}

#[derive(Debug)]
pub struct NodeSignalStats {}

/// The message handling part, but only for nodesignal messages.
#[derive(Debug)]
pub(super) struct Messages {
    our_id: NodeID,
    ts_tx: watch::Sender<NodeSignalStats>,
    nodes: Nodes,
}

#[derive(Debug, Default)]
pub(super) struct Nodes {
    signal_connected: Vec<NodeID>,
    signal_known: Vec<NodeID>,
    node_connected: Vec<NodeID>,
    node_known: HashMap<NodeID, Vec<NodeID>>,
}

impl Messages {
    /// Returns a new chat module.
    pub fn new(our_id: NodeID) -> (Self, watch::Receiver<NodeSignalStats>) {
        let (ts_tx, ts_rx) = watch::channel(NodeSignalStats {});
        (
            Self {
                our_id,
                ts_tx,
                nodes: Nodes::default(),
            },
            ts_rx,
        )
    }

    /// Processes a node to node message and returns zero or more
    /// MessageOut.
    fn process_node_message(&mut self, src: NodeID, msg: ModuleMessage) -> Vec<InternOut> {
        match msg {
            ModuleMessage::NeighborRequest => {
                if let Ok(msg_str) = serde_yaml::to_string(&msg) {
                    vec![InternOut::NetworkNode(NetworkIn::MessageToNode(
                        src, msg_str,
                    ))]
                } else {
                    vec![]
                }
            }
            ModuleMessage::NeighborIDs(ids) => {
                self.nodes.node_known.insert(src, ids.0);
                vec![]
            }
            ModuleMessage::WSMessage(_) => todo!(),
            ModuleMessage::NetworkWrapper(_) => todo!(),
        }
    }

    fn msg_router(&mut self, msg: RouterIn) -> Vec<InternOut> {
        match msg {
            RouterIn::NetworkWrapperToNetwork(u256, network_wrapper) => vec![],
            _ => vec![],
        }
    }

    fn msg_network_signal(&mut self, msg: NetworkOut) -> Vec<InternOut> {
        vec![]
    }

    fn msg_network_node(&mut self, msg: NetworkOut) -> Vec<InternOut> {
        match msg {
            NetworkOut::MessageFromNode(u256, msg) => todo!(),
            NetworkOut::Connected(u256) => todo!(),
            NetworkOut::Disconnected(u256) => todo!(),
            NetworkOut::WebSocket(wsclient_in) => todo!(),
            NetworkOut::WebRTC(web_rtcconn_input) => todo!(),
            _ => vec![]
        }
    }

    fn msg_websocket(&mut self, msg: WSClientOut) -> Vec<InternOut> {
        vec![]
    }

    fn msg_webrtc(&mut self, msg: WebRTCConnOutput) -> Vec<InternOut> {
        vec![]
    }
}

#[platform_async_trait()]
impl SubsystemHandler<InternIn, InternOut> for Messages {
    async fn messages(&mut self, msgs: Vec<InternIn>) -> Vec<InternOut> {
        msgs.into_iter()
            .flat_map(|msg| match msg {
                InternIn::Router(router_in) => self.msg_router(router_in),
                InternIn::NetworkSignal(network_out) => self.msg_network_signal(network_out),
                InternIn::NetworkNode(network_out) => self.msg_network_node(network_out),
                InternIn::WebSocket(wsclient_out) => self.msg_websocket(wsclient_out),
                InternIn::WebRTC(web_rtcconn_output) => self.msg_webrtc(web_rtcconn_output),
            })
            .collect::<Vec<_>>()
    }
}

#[cfg(test)]
mod tests {}
