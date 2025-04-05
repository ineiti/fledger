use std::collections::HashMap;

use flarch::{broker::Broker, nodeids::NodeID, web_rtc::websocket::BrokerWSServer};

use super::signal::{BrokerSignal, SignalConfig, SignalServer};

pub struct NodeSignal {
    ws_broker: BrokerWSServer,
    signal: BrokerSignal,
    nodes: HashMap<NodeID, NodeStat>,
}

impl NodeSignal {
    pub async fn start() -> anyhow::Result<Self> {
        let ws_broker = Broker::new();
        Ok(NodeSignal {
            signal: SignalServer::new(
                ws_broker.clone(),
                SignalConfig {
                    ttl_minutes: 2,
                    system_realm: None,
                    max_list_len: None,
                },
            )
            .await?,
            nodes: HashMap::new(),
            ws_broker,
        })
    }
}

pub enum NodeStat {
    Disconnected,
    NodeSignal(NodeID),
    WebSocket,
}
