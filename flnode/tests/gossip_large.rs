use flarch::{
    data_storage::DataStorageTemp,
    start_logging_filter_level,
    tasks::wait_ms,
    web_rtc::{
        connection::ConnectionConfig, web_socket_server::WebSocketServer, websocket::WSServerIn,
    },
};
use flmodules::{
    network::{
        network_start,
        signal::{BrokerSignal, SignalConfig, SignalIn, SignalOut, SignalServer},
    },
    nodeconfig::NodeConfig,
    Modules,
};
use flnode::node::Node;
use tokio::sync::watch;

/**
 * This test sets up a signalling server, and then lets nodes connect through
 * WebSocket / WebRTC directly to it.
 * The advantage over a test in bash is that it runs with the rust test
 * framework, and is self-contained.
 * This makes it also easier to test and have a fast test-feedback.
 *
 * Some measurements on a local Mac OSX:
 * 003 nodes ->      9kB
 * 010 nodes ->     79kB
 * 100 nodes ->  7_000kB
 * 150 nodes -> 41_000kB
 */

#[tokio::test(flavor = "multi_thread", worker_threads = 16)]
async fn gossip_large() -> anyhow::Result<()> {
    start_logging_filter_level(vec!["fl", "gossip_large"], log::LevelFilter::Debug);

    log::info!("This uses a lot of connections. So be sure to run the following:");
    log::info!("sudo sysctl -w kern.maxfiles=2048000");
    log::info!("sudo sysctl -w kern.maxfilesperproc=1048000");
    log::info!("ulimit -S -n 1000000");

    let mut signal = start_signal().await?;
    let signal_rx = signal.broker.get_tap_out_sync().await?;
    let nbr_nodes: usize = 100;
    let mut nodes = vec![];
    for i in 0..nbr_nodes {
        nodes.push(start_node(i).await?);
    }

    signal_rx
        .0
        .iter()
        .filter(|out| matches!(out, SignalOut::NewNode(_)))
        .take(nbr_nodes)
        .count();

    nodes[0].add_chat_message("something".into()).await?;

    let mut count = 1;
    loop {
        let rx: usize = nodes
            .iter()
            .map(|n| n.gossip.as_ref().unwrap().chat_events().len())
            .sum();
        log::info!(
            "{count:3} - used {:9} bytes to get the message in {} nodes",
            *signal.rx.borrow(),
            rx
        );
        if rx == nbr_nodes {
            break;
        }

        count += 1;
        wait_ms(500).await;
    }

    log::info!("Signal sent {} bytes over websocket", *signal.rx.borrow());
    signal.broker.emit_msg_in(SignalIn::Stop)?;
    for msg in signal_rx.0 {
        if msg == SignalOut::Stopped {
            break;
        }
    }
    signal.broker.remove_subsystem(signal_rx.1).await?;
    Ok(())
}

async fn start_node(i: usize) -> anyhow::Result<Node> {
    let mut nc = NodeConfig::new();
    nc.info.name = format!("{i:2}");
    // nc.info.modules = Modules::all();
    nc.info.modules = Modules::all() - Modules::PING;
    let net = network_start(
        nc.clone(),
        ConnectionConfig::from_signal("ws://localhost:8765"),
    )
    .await?;
    Node::start(Box::new(DataStorageTemp::new()), nc, net.broker).await
}

struct Signal {
    broker: BrokerSignal,
    rx: watch::Receiver<u64>,
}

async fn start_signal() -> anyhow::Result<Signal> {
    let mut signal_server = SignalServer::new(
        WebSocketServer::new(8765).await?,
        SignalConfig {
            ttl_minutes: 2,
            system_realm: None,
        },
    )
    .await?;

    let msgs = signal_server.get_tap_out_sync().await.expect("Getting tap");
    let (tx, rx) = watch::channel(0);
    let mut ss = signal_server.clone();
    tokio::spawn(async move {
        let mut total = 0;
        for msg in msgs.0 {
            log::trace!("Signal out: {msg:?}");
            match msg {
                SignalOut::WSServer(WSServerIn::Message(_, m)) => {
                    total += m.len() as u64;
                    tx.send(total).expect("sending tx size")
                }
                SignalOut::Stopped => ss.remove_subsystem(msgs.1).await.expect("remove subsystem"),
                _ => {}
            }
        }
    });

    Ok(Signal {
        broker: signal_server,
        rx,
    })
}
