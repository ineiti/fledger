/// TextMessages is the structure that holds all known published TextMessages.

use anyhow::Result;
use log::trace;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::mpsc::Sender};
use thiserror::Error;

use super::{messages::{MessageV1, Message}, LOutput};
use crate::{
    node::config::{NodeConfig, NodeInfo},
    types::{now, U256},
};

pub struct TextMessages {
    messages: HashMap<U256, TextMessage>,
    nodes: Vec<NodeInfo>,
    out: Sender<LOutput>,
    // maps a message-id to a list of nodes.
    nodes_msgs: HashMap<U256, Vec<U256>>,
    cfg: NodeConfig,
}

#[derive(Error, Debug)]
enum TMError {
    #[error("Received an unknown message")]
    UnknownMessage,
}

impl TextMessages {
    pub fn new(out: Sender<LOutput>, cfg: NodeConfig) -> Self {
        Self {
            messages: HashMap::new(),
            nodes_msgs: HashMap::new(),
            nodes: vec![],
            out,
            cfg,
        }
    }

    pub fn handle_msg(&mut self, from: &U256, msg: TextMessageV1) -> Result<()> {
        trace!(
            "{}: Handle send from {:?} with {:?}",
            self.cfg.our_node.info,
            from,
            &msg
        );
        match msg {
            TextMessageV1::List() => {
                let ids = self
                    .nodes_msgs
                    .clone()
                    .into_iter()
                    .map(|(id, nodes)| TextStorage { id, nodes })
                    .collect();
                self.send(from, TextMessageV1::IDs(ids))
            }
            TextMessageV1::Get(id) => {
                if let Some(text) = self.messages.get(&id) {
                    // Suppose that the other node will also have it, so add it to the nodes_msgs
                    let nm =
                        self.nodes_msgs
                            .entry(id)
                            .or_insert(vec![self.cfg.our_node.get_id()]);
                    nm.push(from.clone());
                    self.send(from, TextMessageV1::Text(text.clone()))
                } else {
                    Err(TMError::UnknownMessage.into())
                }
            }
            TextMessageV1::Set(text) => {
                // Create a list of nodes, starting with the sender node.
                let mut list = vec![from.clone()];
                for node in self.nodes.iter().filter(|n| &n.get_id() != from) {
                    // Send the text to all nodes other than the sender node and ourselves.
                    self.send(&node.get_id(), TextMessageV1::Text(text.clone()))?;
                    list.push(node.get_id().clone());
                }
                list.push(self.cfg.our_node.get_id().clone());
                self.nodes_msgs.insert(text.id(), list);
                self.messages.insert(text.id(), text);
                Ok(())
            }
            // Currently we just suppose that there is a central node storing all node-ids
            // and texts, so we can simply overwrite the ones we already have.
            TextMessageV1::IDs(list) => {
                // Only ask messages we don't have yet.
                for ts in list.iter().filter(|ts| !self.messages.contains_key(&ts.id)) {
                    // TODO: also send request to other nodes - the first one might not
                    // be responding
                    self.send(
                        ts.nodes.get(0).unwrap(),
                        TextMessageV1::Get(ts.id.clone()),
                    )?;
                }
                Ok(())
            }
            TextMessageV1::Text(tm) => {
                self.messages.insert(tm.id(), tm);
                Ok(())
            }
        }
    }

    /// Updates all known nodes. Will send out requests to new nodes to know what
    /// messages are available in those nodes.
    /// Only nodes different from this one will be stored.
    /// Only new leaders will be asked for new messages.
    pub fn update_nodes(&mut self, nodes: Vec<NodeInfo>) -> Result<()> {
        trace!("{} update_nodes", self.cfg.our_node.info);
        let new_nodes: Vec<NodeInfo> = nodes
            .iter()
            .filter(|n| n.get_id() != self.cfg.our_node.get_id())
            .cloned()
            .collect();

        // Contact only new leaders
        for leader in new_nodes
            .iter()
            .filter(|n| !self.nodes.contains(n))
            .filter(|n| n.node_capacities.leader == true)
        {
            self.send(&leader.get_id(), TextMessageV1::List())?;
        }

        // Store new nodes, overwrite previous nodes
        trace!("new nodes are: {:?}", new_nodes);
        self.nodes = new_nodes;
        Ok(())
    }

    /// Asks the leaders for new messages.
    pub fn update_messages(&mut self) -> Result<()> {
        for leader in self.get_leaders() {
            self.send(&leader.get_id(), TextMessageV1::List())?;
        }
        Ok(())
    }

    /// Adds a new message to the list of messages and sends it to the leaders.
    pub fn add_message(&mut self, msg: String) -> Result<()> {
        let tm = TextMessage {
            src: self.cfg.our_node.get_id(),
            created: now(),
            liked: 0,
            msg,
        };
        self.messages.insert(tm.id(), tm.clone());

        // let mut ret = vec![];
        for leader in self.get_leaders() {
            self.send(&leader.get_id(), TextMessageV1::Set(tm.clone()))?;
        }
        Ok(())
    }

    fn get_leaders(&self) -> Vec<NodeInfo> {
        self.nodes
            .iter()
            .filter(|n| {
                n.node_capacities.leader
            })
            .cloned()
            .collect()
    }

    // Wrapper call to send things over the LOutput
    fn send(&self, to: &U256, msg: TextMessageV1) -> Result<()> {
        let m = Message::V1(MessageV1::TextMessage(msg));
        let str = serde_json::to_string(&m)?;
        self.out
            .send(LOutput::WebRTC(to.clone(), str))
            .map_err(|e| e.into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TextMessageV1 {
    // Requests the updated list of all TextIDs available. This is best
    // sent to one of the Oracle Servers.
    List(),
    // Request a text from a node. If the node doesn't have this text
    // available, it should respond with an Error.
    Get(U256),
    // Stores a new text on the Oracle Servers
    Set(TextMessage),
    // Tuples of [NodeID ; TextID] indicating texts and where they should
    // be read from.
    // The NodeID can change from one TextIDsGet call to another, as nodes
    // can be coming and going.
    IDs(Vec<TextStorage>),
    // The Text as known by the node.
    Text(TextMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextStorage {
    pub id: U256,
    pub nodes: Vec<U256>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextMessage {
    pub src: U256,
    pub created: f64,
    pub liked: u64,
    pub msg: String,
}

impl TextMessage {
    pub fn id(&self) -> U256 {
        let mut id = Sha256::new();
        id.update(&self.src);
        id.update(&self.created.to_le_bytes());
        id.update(&self.liked.to_le_bytes());
        id.update(&self.msg);
        id.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use anyhow::{anyhow, Result};
    use flexi_logger::LevelFilter;
    use log::{debug, info, warn};
    use std::sync::mpsc::{channel, Receiver, TryRecvError};

    use super::{Message, TextMessage, TextMessages};
    use crate::{
        node::{
            config::NodeConfig,
            logic::{messages::MessageV1, LOutput},
        },
        types::U256,
    };

    struct TMTest {
        cfg: NodeConfig,
        queue: Receiver<LOutput>,
        tm: TextMessages,
    }

    impl TMTest {
        pub fn new(leader: bool) -> Result<Self> {
            let (tx, queue) = channel::<LOutput>();
            let mut cfg = NodeConfig::new();
            cfg.our_node.node_capacities.leader = Some(leader);
            let tm = TextMessages::new(tx, cfg.clone());
            Ok(TMTest { cfg, queue, tm })
        }
    }

    /// Processes all messages from all nodes until no messages are left.
    fn process(tms: &mut Vec<&mut TMTest>) -> Result<()> {
        loop {
            let mut msgs: Vec<(U256, U256, String)> = vec![];
            for tm in tms.iter() {
                for msg in tm.queue.try_iter() {
                    match msg {
                        LOutput::WebRTC(to, m) => msgs.push((tm.cfg.our_node.get_id(), to, m)),
                        _ => warn!("Unsupported message received"),
                    }
                }
            }

            if msgs.len() == 0 {
                debug!("No messages found - stopping");
                break;
            }

            for (from, to, s) in msgs {
                let msg: Message = serde_json::from_str(&s)?;
                if let Some(tm) = tms.into_iter().find(|tm| tm.cfg.our_node.get_id() == to) {
                    debug!("Got message {:?} for {}", s, to);
                    match msg {
                        Message::V1(msg_send) => match msg_send {
                            MessageV1::TextMessage(smv) => tm.tm.handle_msg(&from, smv)?,
                            _ => warn!("Ignoring message {:?}", msg_send),
                        },
                        Message::Unknown(s) => warn!("Got Message::Unknown({})", s),
                    }
                } else {
                    warn!("Got message for unknown node {:?}", to)
                }
            }
        }
        Ok(())
    }

    #[test]
    fn test_new_msg() -> Result<()> {
        simple_logging::log_to_stderr(LevelFilter::Trace);

        let mut leader = TMTest::new(true)?;
        let mut follower1 = TMTest::new(false)?;
        let mut follower2 = TMTest::new(false)?;

        let all_nodes = vec![
            leader.cfg.our_node.clone(),
            follower1.cfg.our_node.clone(),
            follower2.cfg.our_node.clone(),
        ];

        assert_eq!(leader.tm.messages.len(), 0);
        assert_eq!(follower1.tm.messages.len(), 0);
        assert_eq!(follower2.tm.messages.len(), 0);

        info!("Adding a first message to the follower 1");
        let msg1 = String::from("1st Message");
        follower1.tm.add_message(msg1.clone())?;
        if !matches!(follower1.queue.try_recv(), Err(TryRecvError::Empty)) {
            panic!("queue should be empty");
        }

        // Add a new message to follower 1 and verify it's stored in the leader
        follower1.tm.update_nodes(all_nodes.clone())?;
        leader.tm.update_nodes(all_nodes[0..2].to_vec())?;
        if matches!(leader.queue.try_recv(), Ok(_)) {
            panic!("leader queue should be empty");
        }
        let msg2 = String::from("2nd Message");
        follower1.tm.add_message(msg2.clone())?;
        process(&mut vec![&mut follower1, &mut leader])?;
        assert_eq!(1, leader.tm.messages.len());
        let tm1_id = leader.tm.messages.keys().next().unwrap().clone();
        assert_eq!(2, leader.tm.nodes_msgs.get(&tm1_id).unwrap().len());

        // Let the follower2 catch up on the new messages
        info!("Follower2 catches up");
        follower2.tm.update_nodes(all_nodes.clone())?;
        process(&mut vec![&mut follower1, &mut follower2, &mut leader])?;
        assert_eq!(follower2.tm.messages.len(), 1);
        assert_eq!(2, follower1.tm.nodes_msgs.get(&tm1_id).unwrap().len());

        // Follower2 also creates their message
        info!("Follower2 creates new message");
        let msg3 = String::from("3rd Message");
        leader.tm.update_nodes(all_nodes.clone())?;
        follower2.tm.add_message(msg3)?;
        process(&mut vec![&mut follower1, &mut follower2, &mut leader])?;
        for msg in &follower1.tm.messages {
            info!("Message is: {:?}", msg.1);
        }
        assert_eq!(follower1.tm.messages.len(), 3);

        Ok(())
    }

    #[test]
    fn test_update_nodes() -> Result<()> {
        simple_logging::log_to_stderr(LevelFilter::Trace);

        let leader = TMTest::new(true)?;
        let mut follower1 = TMTest::new(false)?;

        let all_nodes = vec![leader.cfg.our_node.clone(), follower1.cfg.our_node.clone()];

        // Update nodes twice - first should send a message to the leader,
        // second update_nodes should come out empty, because no new
        // leader is found.
        follower1.tm.update_nodes(all_nodes.clone())?;
        if matches!(follower1.queue.try_recv(), Err(_)) {
            panic!("queue should have one message");
        }
        assert_eq!(1, follower1.tm.nodes.len());

        follower1.tm.update_nodes(all_nodes.clone())?;
        if matches!(follower1.queue.try_recv(), Ok(_)) {
            panic!("queue should be empty now");
        }
        assert_eq!(1, follower1.tm.nodes.len());

        follower1.tm.update_nodes(vec![])?;
        assert_eq!(0, follower1.tm.nodes.len());

        Ok(())
    }

    #[test]
    fn test_id() {
        let tm1 = TextMessage {
            src: U256::rnd(),
            created: 0f64,
            liked: 0u64,
            msg: "test message".to_string(),
        };
        assert_eq!(tm1.id(), tm1.id());

        let mut tm2 = tm1.clone();
        tm2.src = U256::rnd();
        assert_ne!(tm1.id(), tm2.id());

        tm2 = tm1.clone();
        tm2.created = 1f64;
        assert_ne!(tm1.id(), tm2.id());

        tm2 = tm1.clone();
        tm2.liked = 1u64;
        assert_ne!(tm1.id(), tm2.id());

        tm2 = tm1.clone();
        tm2.msg = "short test".to_string();
        assert_ne!(tm1.id(), tm2.id());
    }
}
