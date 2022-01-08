use common::types::U256;
use serde::{Serialize, Deserialize};

use crate::connections::ConnectionsMessage;
use crate::random_connections::RandomConnectionsNodeMessage;
use crate::gossip_chat::GossipChatNodeMessage;
use crate::random_connections::RandomConnectionsMessage;
use crate::gossip_chat::GossipChatMessage;

/// The messages used between the nodes.
/// Intern messages are sent only between nodes.
/// Node2Node messages have a destination and will be sent to the given node.
pub enum Message {
    Intern(Intern),
    Node2Node(Node2Node),
}

pub enum Intern {
    GossipChat(GossipChatMessage),
    RandomConnections(RandomConnectionsMessage),
    Connections(ConnectionsMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node2Node {
    pub id: Address,
    pub msg: Node2NodeMsg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Address {
    From(U256),
    To(U256),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Node2NodeMsg {
    GossipChat(GossipChatNodeMessage),
    RandomConnections(RandomConnectionsNodeMessage),
}

/// This is the common module trait that every module needs to implement.
pub trait Module {
    fn new(ds: Box<dyn DataStorage>) -> Self;

    fn process_message(&mut self, msg: &Message) -> Vec<Message>;

    fn tick(&mut self) -> Vec<Message>;
}

/// A DataStorageBase can give different DataStorages.
/// Each DataStorage must present an independant namespace for `get`
/// and `put`.
pub trait DataStorageBase {
    fn get(&self, base: &str) -> Box<dyn DataStorage>;
}

/// The DataStorage trait allows access to a persistent storage. Each module
/// has it's own DataStorage, so there will never be a name clash.
pub trait DataStorage {
    fn get(&self, key: &str) -> String;

    fn put(&mut self, key: &str, value: &str);
}
