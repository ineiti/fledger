use flarch::start_logging_filter;
use flnet::NetworkSetupError;

use shared::{
    common::NodeList,
    handler::{PingPong, URL},
};

#[tokio::main]
async fn main() -> Result<(), NetworkSetupError> {
    start_logging_filter(vec!["fl", "libc"]);

    let nc = flnet::config::NodeConfig::new();
    let name = nc.info.name.clone();
    let net = flnet::network_start(nc.clone(), URL).await?;
    let mut list = NodeList::new(vec![]);

    let mut pp = PingPong::new(nc.info.get_id(), net).await?;
    let (pp_rx, _) = pp.get_tap().await?;
    for msg in pp_rx {
        match msg {
            shared::common::PPMessage::ToNetwork(to, ppmsg) => {
                log::info!("{name}: Sending {ppmsg:?} to {}", list.get_name(&to))
            }
            shared::common::PPMessage::FromNetwork(from, ppmsg) => {
                log::info!("{name}: Got {ppmsg:?} from {}", list.get_name(&from))
            }
            shared::common::PPMessage::List(new_list) => {
                if list.update(new_list) {
                    log::info!("{name}: Got new list: {}", list.names());
                }
            }
            _ => {}
        }
    }
    Ok(())
}
