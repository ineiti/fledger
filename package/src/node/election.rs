use crate::node::broadcast::{BroadcastFromTabs, BroadcastToTabs};

#[derive(Clone, Debug)]
pub enum ElectionIn {
    Broadcast(BroadcastFromTabs),
    Start(usize),
}

#[derive(Clone, Debug)]
pub enum ElectionOut {
    Broadcast(BroadcastToTabs),
    IsLeader,
    IsFollower,
}
