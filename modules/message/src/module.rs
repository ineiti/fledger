use crate::network::NetworkMessage;
use crate::random_connections::RandomConnectionsMessage;
use crate::gossip_chat::GossipChatMessage;
use types::data_storage::DataStorage;

/// The messages used between the nodes.
pub enum Message {
    GossipChat(GossipChatMessage),
    RandomConnections(RandomConnectionsMessage),
    Network(NetworkMessage),
}

/// This is the common module trait that every module needs to implement.
pub trait Module {
    fn new(ds: Box<dyn DataStorage>) -> Self;

    fn process_message(&mut self, msg: &Message) -> Vec<Message>;

    fn tick(&mut self) -> Vec<Message>;
}
