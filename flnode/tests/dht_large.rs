use flarch::{
    data_storage::DataStorageTemp,
    start_logging_filter_level,
    tasks::wait_ms,
    web_rtc::{
        connection::ConnectionConfig, web_socket_server::WebSocketServer, websocket::WSServerIn,
    },
};
use flcrypto::access::Condition;
use flmodules::{
    dht_storage::realm_view::RealmView,
    network::{
        network_start,
        signal::{
            BrokerSignal, SignalConfig, SignalIn, SignalOut, SignalServer, WSSignalMessageFromNode,
        },
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
 * 003 nodes ->     15kB
 * 010 nodes ->    114kB
 * 030 nodes ->    562kB
 * 100 nodes ->  2_600kB
 * 200 nodes ->  6_000kB
 * 250 nodes ->  8_200kB
 * 300 nodes -> 10_000kB // Already has problems with some of the connections
 */

#[tokio::test(flavor = "multi_thread", worker_threads = 16)]
async fn dht_large() -> anyhow::Result<()> {
    // start_logging_filter_level(
    //     vec!["flarch::web_rtc", "dht_large", "flmodules::dht_storage"],
    //     log::LevelFilter::Trace,
    // );
    start_logging_filter_level(vec!["dht_large", "fl"], log::LevelFilter::Debug);

    log::info!("This uses a lot of connections. So be sure to run the following:");
    log::info!("sudo sysctl -w kern.maxfiles=2048000");
    log::info!("sudo sysctl -w kern.maxfilesperproc=1048000");
    log::info!("ulimit -S -n 1000000");

    let mut signal = start_signal().await?;
    let signal_rx = signal.broker.get_tap_out_sync().await?;
    let nbr_nodes: usize = 100;
    let mut nodes = vec![];
    let mut dhts = vec![];
    for i in 0..nbr_nodes {
        let mut node = start_node(i).await?;
        dhts.push(node.dht_storage.as_mut().unwrap().clone());
        nodes.push(node);
    }

    signal_rx
        .0
        .iter()
        .filter(|out| matches!(out, SignalOut::NewNode(_)))
        .take(nbr_nodes)
        .count();

    let mut connections = 0;
    for node in &mut nodes {
        let connection = node.dht_router.as_ref().unwrap().stats.borrow().active;
        connections += connection;
        node.request_list().await?;
    }
    log::info!("Total Connections: {connections}");

    RealmView::new_create_realm(
        nodes[0].dht_storage.as_mut().unwrap().clone(),
        "root",
        Condition::Pass,
        &[],
    )
    .await?;

    let mut count = 1;
    loop {
        let rx: usize = dhts
            .iter()
            .map(|dht| dht.stats.borrow().realm_stats.len())
            .sum();
        log::info!(
            "{count:3} - used {:9} bytes to get the realm in {} nodes",
            *signal.rx.borrow(),
            rx
        );
        if rx == nbr_nodes {
            break;
        }

        if count % 5 == 0 {
            let connections: (usize, usize) = nodes
                .iter()
                .map(|node| node.dht_router.as_ref().unwrap().stats.borrow())
                .map(|stats| (stats.active, stats.all_nodes.len() - stats.active))
                .fold((0, 0), |(sum_a, sum_b), (a, b)| (sum_a + a, sum_b + b));
            log::info!(
                "Total Connections active / cache: {} / {}",
                connections.0,
                connections.1
            );

            for dht in &mut dhts {
                dht.sync()?;
            }
        }

        count += 1;
        wait_ms(1000).await;
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
    nc.info.modules = Modules::all() - Modules::PING - Modules::RAND - Modules::GOSSIP;
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
            max_list_len: Some(3),
        },
    )
    .await?;

    let msgs = signal_server.get_tap_out_sync().await.expect("Getting tap");
    let (tx, rx) = watch::channel(0);
    let mut ss = signal_server.clone();
    tokio::spawn(async move {
        let mut total = 0;
        for msg in msgs.0 {
            // log::trace!("Signal out: {msg:?}");
            if let SignalOut::WSServer(WSServerIn::Message(ch, msg_str)) = &msg {
                if let Ok(msg_ws) = serde_json::from_str::<WSSignalMessageFromNode>(&msg_str) {
                    if let WSSignalMessageFromNode::PeerSetup(msg_peer) = msg_ws {
                        log::trace!("{}: {} {} -> {}", ch, msg_peer.message, msg_peer.id_init, msg_peer.id_follow);
                    }
                }
            }
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
