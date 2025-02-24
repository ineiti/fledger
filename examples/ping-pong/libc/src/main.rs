use clap::{Parser, ValueEnum};

use flarch::{start_logging_filter, web_rtc::connection::ConnectionConfig};
use flmodules::network::NetworkSetupError;
use flmodules::nodeconfig::NodeConfig;

use shared::{
    common::{NodeList, PingPongIn, PingPongOut},
    event_loop,
    handler::{self, URL},
};
use tokio::select;

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
    let net =
        flmodules::network::network_start(nc.clone(), ConnectionConfig::from_signal(URL))
            .await?;
    let mut list = NodeList::new(vec![]);

    let mut pp = match args.code {
        Code::Handler => handler::PingPong::new(nc.info.get_id(), net.broker).await?,
        Code::EventLoop => event_loop::start(nc.info.get_id(), net.broker).await?,
    };
    let mut pp_rx = pp.get_tap_in().await?.0;
    let mut pp_tx = pp.get_tap_out().await?.0;
    loop {
        select! {
            Some(msg) = pp_rx.recv() => {
                match msg{
                    PingPongIn::FromNetwork(from, ppmsg) => {
                        log::info!("{name}: Got {ppmsg:?} from {}", list.get_name(&from))
                    }
                    PingPongIn::List(new_list) => {
                        if list.update(new_list) {
                            log::info!("{name}: Got new list: {}", list.names());
                        }
                    }
                    _ => {}
                }
            },
            Some(msg) = pp_tx.recv() => {
                match msg {
                    PingPongOut::ToNetwork(to, ppmsg) => {
                        log::info!("{name}: Sending {ppmsg:?} to {}", list.get_name(&to))
                    }
                    _ => {}
                }
            },
            else => break, // Exit when all channels close
        }
    }
    Ok(())
}
