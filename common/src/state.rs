// use super::config::*;

/// V0.1 states

/// Logs is the first structure being held by the server node.
/// It will be replaced by the Accounts, then the blocks.
pub struct Logs {
    // broadcast: Vec<LogConnectivity>
}

pub struct LogConnectivity {
    // time: u32,
// connectivities: Vec<Connectivity>,
}

pub struct Connectivity {
    // source: NodeInfo,
// connections: Vec<Connection>,
}

/// Represents one connection from a node to another.
pub struct Connection {
    // with: NodeInfo,
// ping-time in miliseconds. If not reachable, -1.
// ping: i32,
}
