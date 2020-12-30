use super::config::NodeInfo;
use crate::config::parse_config;

use crate::ext_interface::RestCall;use crate::ext_interface::WebRTCCall;
use std::future::Future;

use crate::rest::RestClient;
use crate::web_rtc::WebRTCClient;

use ext_interface::Logger;
use ext_interface::Storage;

// use super::config::parse_config;
use super::ext_interface;

/// The node structure holds it all together. It is the main structure of the project.
struct Node<'r, F: Future, G: Future> {
    pub info: NodeInfo,
    pub nodes: Vec<NodeInfo>,
    pub rest: RestClient<'r, F>,
    pub web_rtc: WebRTCClient<G>,
    pub storage: Box<dyn Storage>,
    pub logger: Box<dyn Logger>,
}

impl<'r, F, G> Node<'r, F, G>
where
    F: Future<Output = Result<String, String>>,
    G: Future<Output = Result<Option<String>, String>>,
{
    pub fn new(
        rest: RestCall<'r, F>,
        web_rtc: WebRTCCall<G>,
        st: Box<dyn Storage>,
        log: Box<dyn Logger>,
    ) -> Result<Node<'r, F, G>, String> {
        let config = st.load("nodeConfig")?;
        let toml_config = parse_config(config)?;
        Ok(Node {
            info: toml_config.our_node,
            storage: st,
            rest: RestClient::new(rest),
            web_rtc: WebRTCClient::new(web_rtc),
            logger: log,
            nodes: vec![],
        })
    }

    /// TODO: this is only for development
    pub fn clear(&self) -> Result<(), String> {
        Ok(())
    }
}
