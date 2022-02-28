use flnet::{config::NodeConfig, signal::dummy::{WebSocketConnectionDummy, WebSocketSimul}};

#[tokio::test(flavor = "multi_thread")]
async fn flnet() -> Result<(), ()> {
    let _ = env_logger::try_init();

    let mut wsd = WebSocketSimul::new();
    let (i, conn) = wsd.new_connection();

    let init = Node::new(conn);
    Ok(())
}

struct Node {
    config: NodeConfig,
    ws: WebSocketConnectionDummy,
}

impl Node {
    fn new(ws: WebSocketConnectionDummy) -> Self {
        let config = NodeConfig::new();
        Self { config, ws }
    }
}
