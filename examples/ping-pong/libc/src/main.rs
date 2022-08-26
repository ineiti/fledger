use std::time::Duration;
use tokio::time::sleep;

use flarch::start_logging_filter;
use flmodules::nodeids::U256;
use flnet::NetworkSetupError;

use shared::{PPMessage, PPMessageNode, PingPong};

#[tokio::main]
async fn main() -> Result<(), NetworkSetupError> {
    start_logging_filter(vec!["fl", "libc"]);
    // start_logging_filter(vec![]);

    let nc = flnet::config::NodeConfig::new();
    let id = nc.info.get_id();
    let net = flnet::network_start(nc, shared::URL).await?;

    let mut pp = PingPong::new(id, net).await?;
    let (pp_rx, _) = pp.get_tap().await?;
    let mut list: Vec<U256> = vec![];

    log::info!("Node started with id {}", id);
    
    loop {
        for node in &list {
            if node != &id {
                log::info!("Sending ping to {}", node);
                pp.emit_msg(PPMessage::ToNetwork(node.clone(), PPMessageNode::Ping))
                    .await?;
            }
        }
        for msg in pp_rx.try_iter() {
            match msg {
                PPMessage::FromNetwork(from, pm) => {
                    log::info!("Got {pm:?} from {from}");
                }
                PPMessage::List(l) => {
                    log::info!(
                        "Got new list {:?}",
                        l.iter().map(|n| format!("{}", n)).collect::<Vec<String>>()
                    );
                    list = l;
                }
                _ => {}
            }
        }
        sleep(Duration::from_millis(1000)).await;
        log::info!("");
    }
}
