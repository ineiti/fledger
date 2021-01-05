use crate::config::{NodeConfig, NodeInfo};


use crate::ext_interface::RestCaller;use crate::ext_interface::WebRTCCaller;use crate::ext_interface::{DataStorage, Logger};
use crate::rest::RestClient;
use crate::web_rtc::WebRTCClient;

/// The node structure holds it all together. It is the main structure of the project.
pub struct Node {
    pub info: NodeInfo,
    pub nodes: Vec<NodeInfo>,
    pub rest: RestClient,
    pub web_rtc: WebRTCClient,
    pub storage: Box<dyn DataStorage>,
    pub logger: Box<dyn Logger>,
}

const CONFIG_NAME: &str = "nodeConfig";

impl Node {
    /// Create new node by loading the config from the storage.
    /// If the storage is
    pub fn new<'a>(
        rest: Box<dyn RestCaller>,
        web_rtc: Box<dyn WebRTCCaller>,
        storage: Box<dyn DataStorage>,
        logger: Box<dyn Logger>,
    ) -> Result<Node, String> {
        let config = NodeConfig::new(storage.load(CONFIG_NAME)?)?;
        logger.info("Config loaded");
        storage.save(CONFIG_NAME, &config.to_string()?)?;
        logger.info("Config saved");
        logger.info(&format!("Starting node: {:?}", config.our_node.public));

        Ok(Node {
            info: config.our_node,
            storage,
            rest: RestClient::new(rest),
            web_rtc: WebRTCClient::new(web_rtc),
            logger,
            nodes: vec![],
        })
    }

    /// TODO: this is only for development
    pub async fn clear(&self) -> Result<(), String> {
        self.rest.clear_nodes().await
    }

    pub async fn connect(&mut self) {
        self.logger.info("Connecting to server");
        if let Ok(id) = self.rest.new_id().await {
            if self.rest.add_id(id, &self.info).await.is_ok() {
                self.logger.info("Successfully announced at server");
            }
        }
    }
}
