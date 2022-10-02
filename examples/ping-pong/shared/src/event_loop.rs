use tokio::select;

use flarch::wait_ms;
use flmodules::broker::{Broker, BrokerError};
use flnet::{
    config::NodeInfo,
    network::{NetCall, NetworkMessage},
};

pub async fn start(mut net: Broker<NetworkMessage>) -> Result<(), BrokerError> {
    let (mut tap, _) = net.get_tap_async().await?;
    let mut nodes: Vec<NodeInfo> = vec![];
    loop {
        select! {
        () = wait_ms(1000) => update_list(&mut net, &nodes)?,
            msg = tap.recv() => if let Some(list) = new_msg(&mut net, msg)?{
                nodes = list;
            },
        }
    }
}

fn update_list(net: &mut Broker<NetworkMessage>, nodes: &Vec<NodeInfo>) -> Result<(), BrokerError> {
    net.emit_msg(NetworkMessage::Call(NetCall::SendWSUpdateListRequest))?;
    for node in nodes.iter() {
        net.emit_msg(NetworkMessage::Call(NetCall::SendNodeMessage(
            node.get_id(),
            "Ping".into(),
        )))?;
    }

    Ok(())
}

fn new_msg(
    net: &mut Broker<NetworkMessage>,
    msg: Option<NetworkMessage>,
) -> Result<Option<Vec<NodeInfo>>, BrokerError> {
    if let Some(NetworkMessage::Reply(msg_tap)) = msg {
        match msg_tap {
            flnet::network::NetReply::RcvNodeMessage(from, msg_net) => match msg_net.as_str() {
                "Ping" => net.emit_msg(NetworkMessage::Call(NetCall::SendNodeMessage(
                    from,
                    "Pong".into(),
                )))?,
                "Pong" => log::info!("Got 'Pong' from {from}"),
                _ => log::info!("Unknown message {msg_net}"),
            },
            flnet::network::NetReply::RcvWSUpdateList(list) => return Ok(Some(list)),
            _ => {}
        }
    } else {
        return Ok(None);
    }
    Ok(None)
}
