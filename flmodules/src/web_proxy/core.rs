use std::collections::HashMap;
use std::collections::HashSet;

use bytes::Bytes;
use flarch::tasks::spawn_local;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use flarch::nodeids::{NodeID, NodeIDs, U256};

use super::response::{ResponseHeader, ResponseMessage};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct WebProxyConfig {
    node: Option<NodeID>,
}

impl Default for WebProxyConfig {
    fn default() -> Self {
        Self { node: None }
    }
}

#[derive(Debug)]
pub struct WebProxyCore {
    pub storage: WebProxyStorage,
    pub config: WebProxyConfig,
    nodes: NodeIDs,
    our_id: NodeID,
    node_index: usize,
    requests: HashMap<U256, (NodeID, Sender<Bytes>)>,
    responded_nonces: HashSet<U256>,
}

impl WebProxyCore {
    /// Initializes a new Proxy.
    pub fn new(storage: WebProxyStorage, config: WebProxyConfig, our_id: NodeID) -> Self {
        Self {
            storage,
            config,
            nodes: NodeIDs::empty(),
            node_index: 0,
            requests: HashMap::new(),
            our_id,
            responded_nonces: HashSet::new(),
        }
    }

    pub fn start_request(&mut self, request: String, tx: Sender<Bytes>, count: usize) {
        for _ in 0..count {
            let tx_clone = tx.clone();
            let request_clone = request.clone();
            spawn_local(async move {
                println!("{request_clone}");
                if let Err(e) = tx_clone.send(Bytes::from("something")).await {
                    log::error!("Failed to send reply: {:?}", e);
                }
            });
        }
    }

    pub fn node_list(&mut self, mut nodes: NodeIDs) {
        self.nodes = nodes.remove_missing(&vec![self.our_id].into());
    }

    pub fn get_node(&mut self) -> Option<NodeID> {
        if self.nodes.0.len() == 0 {
            return None;
        }
        self.node_index %= self.nodes.0.len();
        let node = self.nodes.0.get(self.node_index).unwrap();
        self.node_index += 1;
        Some(*node)
    }

    pub fn request_get(&mut self, rnd: U256, tx: Sender<Bytes>) -> Option<NodeID> {
        if let Some(node) = self.get_node() {
            self.requests.insert(rnd, (node, tx));
            return Some(node);
        }
        None
    }

    pub fn handle_response(&mut self, nonce: U256, msg: ResponseMessage) -> Option<ResponseHeader> {
        if let Some((_, tx)) = self.requests.get(&nonce) {
            match msg {
                ResponseMessage::Header(header) => {
                    self.storage.counters.rx_packets += 1;
                    return Some(header);
                }
                ResponseMessage::Body(body) => {
                    self.storage.counters.rx_packets += 1;
                    let tx = tx.clone();
                    spawn_local(async move { 
                        if let Err(e) = tx.send(body).await {
                            log::error!("Failed to send body: {:?}", e);
                        }
                    })
                }
                ResponseMessage::Done => {
                    self.requests.remove(&nonce);
                }
                ResponseMessage::Error(err) => {
                    log::warn!("Got error {err} for response of nonce {nonce}")
                }
            }
        }
        None
    }

    pub fn has_responded(&self, nonce: U256) -> bool {
        self.responded_nonces.contains(&nonce)
    }

    pub fn mark_as_responded(&mut self, nonce: U256) {
        self.responded_nonces.insert(nonce);
    }

}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum WebProxyStorageSave {
    V1(WebProxyStorage),
}

impl WebProxyStorageSave {
    pub fn from_str(data: &str) -> Result<WebProxyStorage, serde_yaml::Error> {
        return Ok(serde_yaml::from_str::<WebProxyStorageSave>(data)?.to_latest());
    }

    fn to_latest(self) -> WebProxyStorage {
        match self {
            WebProxyStorageSave::V1(es) => es,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Counters {
    pub rx_requests: u32,
    pub tx_requests: u32,
    pub rx_packets: u32,
    pub tx_packets: u32,
}

impl Default for Counters {
    fn default() -> Self {
        Self {
            rx_requests: 0,
            tx_requests: 0,
            rx_packets: 0,
            tx_packets: 0,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct WebProxyStorage {
    pub counters: Counters,
}

impl WebProxyStorage {
    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string::<WebProxyStorageSave>(&WebProxyStorageSave::V1(self.clone()))
    }
}

impl Default for WebProxyStorage {
    fn default() -> Self {
        Self {
            counters: Counters::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    // use super::*;

    #[test]
    fn test_increase() -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}
