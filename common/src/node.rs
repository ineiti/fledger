use super::config::NodeInfo;
use crate::config::parse_config;

use crate::rest::RestClient;
use crate::web_rtc::WebRTCClient;

use ext_interface::Logger;
use ext_interface::Storage;

// use super::config::parse_config;
use super::ext_interface;

/// The node structure holds it all together. It is the main structure of the project.
struct Node {
    pub info: NodeInfo,
    pub nodes: Vec<NodeInfo>,
    pub rest: RestClient,
    pub web_rtc: WebRTCClient,
    pub storage: Box<dyn Storage>,
    pub logger: Box<dyn Logger>,
}

impl Node {
    pub fn new(
        rest: RestClient,
        web_rtc: WebRTCClient,
        st: Box<dyn Storage>,
        log: Box<dyn Logger>,
    ) -> Result<Node, String> {
        let config = st.load("nodeConfig")?;
        let toml_config = parse_config(config)?;
        Ok(Node {
            info: toml_config.our_node,
            storage: st,
            rest,
            web_rtc,
            logger: log,
            nodes: vec![],
        })
    }

    /// TODO: this is only for development
    pub fn clear(&self) -> Result<(), String> {
        Ok(())
    }
}
