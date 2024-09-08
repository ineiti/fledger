use clap::{Parser, ValueEnum};

use flarch::{start_logging_filter, web_rtc::connection::ConnectionConfig};
use flmodules::nodeconfig::NodeConfig;
use flnet::NetworkSetupError;

use shared::{
    common::NodeList,
    event_loop,
    handler::{self, URL},
};

#[derive(Parser, Debug)]
struct Args {
    /// Which shared code to run
    #[arg(value_enum, short, long, default_value_t = Code::Handler)]
    code: Code,
}

#[derive(ValueEnum, Clone, Debug)]
enum Code {
    Handler,
    EventLoop,
}

#[tokio::main]
async fn main() -> Result<(), NetworkSetupError> {
    start_logging_filter(vec!["fl", "libc"]);
    let args = Args::parse();

    let nc = NodeConfig::new();
    let name = nc.info.name.clone();
    let net = flnet::network_broker_start(nc.clone(), ConnectionConfig::from_signal(URL)).await?;
    let mut list = NodeList::new(vec![]);

    let mut pp = match args.code {
        Code::Handler => handler::PingPong::new(nc.info.get_id(), net).await?,
        Code::EventLoop => event_loop::start(nc.info.get_id(), net).await?,
    };
    let (pp_rx, _) = pp.get_tap_sync().await?;
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
