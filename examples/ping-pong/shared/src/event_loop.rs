use std::time::Duration;
use tokio::select;
use tokio_stream::StreamExt;

use flarch::{
    broker::{BrokerError, Broker},
    nodeids::NodeID,
    tasks::{spawn_local, Interval},
};
use flmodules::network::broker::{BrokerNetwork, NetworkIn, NetworkOut};
use flmodules::nodeconfig::NodeInfo;

use crate::common::{PPMessageNode, PingPongIn, PingPongOut};

/// This is a more straightforward use of the network-broker. It loops over incoming
/// messages and then sends out new messages by sending it over the network-broker.
/// To keep compatibility with the handler implementation, it uses a Broker<PPMessage>
/// as a simple channel.
pub async fn start(
    id: NodeID,
    net: BrokerNetwork,
) -> Result<Broker<PingPongIn, PingPongOut>, BrokerError> {
    let ret = Broker::new();

    let ret_clone = ret.clone();
    spawn_local(async move {
        event_loop(id, net, ret_clone)
            .await
            .expect("Event loop failed")
    });

    Ok(ret)
}

/// Fetches messages from the network broker and requests update for the node-list
/// once a second.
async fn event_loop(
    id: NodeID,
    mut net: BrokerNetwork,
    mut ret: Broker<PingPongIn, PingPongOut>,
) -> Result<(), BrokerError> {
    let (mut tap, _) = net.get_tap_out().await?;
    let mut nodes: Vec<NodeInfo> = vec![];
    let mut interval_sec = Interval::new_interval(Duration::from_secs(1));

    loop {
        select! {
            msg = tap.recv() => if let Some(list) = new_msg(&mut net, &mut ret, msg)?{
                nodes = list;
            },
            _ = interval_sec.next() => update_list(id, &mut net, &nodes)?,
        }
    }
}

/// Requests a new list and sends a ping to all other nodes.
fn update_list(
    id: NodeID,
    net: &mut BrokerNetwork,
    nodes: &Vec<NodeInfo>,
) -> Result<(), BrokerError> {
    net.emit_msg_in(NetworkIn::WSUpdateListRequest)?;
    for node in nodes.iter() {
        if node.get_id() != id {
            net.emit_msg_in(NetworkIn::MessageToNode(
                node.get_id(),
                serde_json::to_string(&PPMessageNode::Ping).unwrap(),
            ))?;
        }
    }

    Ok(())
}

fn new_msg(
    net: &mut BrokerNetwork,
    ret: &mut Broker<PingPongIn, PingPongOut>,
    msg: Option<NetworkOut>,
) -> Result<Option<Vec<NodeInfo>>, BrokerError> {
    if let Some(msg_tap) = msg {
        match msg_tap {
            NetworkOut::MessageFromNode(from, msg_net) => {
                if let Ok(msg) = serde_json::from_str::<PPMessageNode>(&msg_net) {
                    ret.emit_msg_in(PingPongIn::FromNetwork(from, msg.clone()))?;
                    if msg == PPMessageNode::Ping {
                        net.emit_msg_in(NetworkIn::MessageToNode(
                            from,
                            serde_json::to_string(&PPMessageNode::Pong).unwrap(),
                        ))?;
                    }
                }
            }
            NetworkOut::NodeListFromWS(list) => {
                ret.emit_msg_in(PingPongIn::List(list.clone()))?;
                return Ok(Some(list));
            }
            _ => {}
        }
    } else {
        return Ok(None);
    }
    Ok(None)
}
