use crate::modules::NodeID;
use crate::modules::NodeIDs;
use rand::seq::SliceRandom;

/// RandomConnections listens for new available nodes and then chooses
/// to randomly connect to a set number of nodes.
pub struct Module {
    cfg: Config,
    nodes: Nodes,
    all_nodes: NodeIDs,
}

#[derive(PartialEq, Clone, Debug)]
struct NodeTime {
    id: NodeID,
    ticks: u32,
}

enum NodeStatus {
    Connected(NodeTime),
    Connecting(NodeTime),
}

impl NodeStatus {
    fn get_connected(&self) -> Option<NodeTime> {
        match self {
            NodeStatus::Connected(nt) => Some(nt.clone()),
            _ => None,
        }
    }
    fn get_connecting(&self) -> Option<NodeTime> {
        match self {
            NodeStatus::Connecting(nt) => Some(nt.clone()),
            _ => None,
        }
    }
    fn get_node_time(&self) -> NodeTime {
        match self {
            NodeStatus::Connecting(nt) => nt.clone(),
            NodeStatus::Connected(nt) => nt.clone(),
        }
    }
}

struct Nodes(Vec<NodeStatus>);

impl Nodes {
    fn new() -> Nodes {
        Self { 0: vec![] }
    }

    fn get_connected(&self) -> Vec<NodeTime> {
        self.0.iter().filter_map(|ns| ns.get_connected()).collect()
    }

    fn get_connecting(&self) -> Vec<NodeTime> {
        self.0.iter().filter_map(|ns| ns.get_connecting()).collect()
    }

    fn get_connected_nodes(&self) -> NodeIDs {
        NodeIDs {
            0: self
                .get_connected()
                .iter()
                .map(|nt| nt.id.clone())
                .collect(),
        }
    }

    fn get_connecting_nodes(&self) -> NodeIDs {
        NodeIDs {
            0: self
                .get_connecting()
                .iter()
                .map(|nt| nt.id.clone())
                .collect(),
        }
    }

    fn get_node_times(&self, nodes: NodeIDs) -> Vec<NodeTime> {
        nodes
            .0
            .iter()
            .map(|n| {
                self.0
                    .iter()
                    .map(|ns| ns.get_node_time())
                    .find(|nt| nt.id == *n)
                    .unwrap_or(NodeTime {
                        id: n.clone(),
                        ticks: 0,
                    })
            })
            .collect()
    }

    fn update(&mut self, connecting: NodeIDs, connected: NodeIDs) {
        self.0 = self
            .get_node_times(connecting)
            .iter()
            .map(|c| NodeStatus::Connecting(c.clone()))
            .chain(
                self.get_node_times(connected)
                    .iter()
                    .map(|c| NodeStatus::Connected(c.clone())),
            )
            .collect();
    }

    fn connect(&mut self, node: &NodeID) {
        for n in self.0.iter_mut() {
            let nt = n.get_node_time();
            if nt.id == *node {
                *n = NodeStatus::Connected(NodeTime { id: nt.id, ticks: 0 });
                break;
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Message {
    /// SetupConnections returns a list of nodes to connect to,
    /// and a list of nodes to disconnect from.
    SetupConnections(NodeIDs, NodeIDs),

    SomethingElse(),
}

pub struct Config {
    /// How many maximum connections the system tries to make
    pub max_connections: u32,

    /// How many ticks a node stays in the list before it is
    /// possibly replaced by another node.
    pub churn_tick: u32,
}

impl Config {
    pub fn default() -> Self {
        Config {
            max_connections: 2,
            churn_tick: 60,
        }
    }
}

impl Module {
    pub fn new(cfg: Option<Config>) -> Self {
        Module {
            cfg: cfg.unwrap_or(Config::default()),
            nodes: Nodes::new(),
            all_nodes: NodeIDs::new(0),
        }
    }

    /// When a new list of nodes is available.
    /// returns the list of nodes to (dis)connect to.
    pub fn new_nodes(&mut self, nodes: &NodeIDs) -> Message {
        self.all_nodes = nodes.clone();
        // Search for nodes that are not available anymore
        // and that should be disconnected.
        let mut connecting = self.nodes.get_connecting_nodes();
        let mut connected = self.nodes.get_connected_nodes();
        let mut disconnect = connecting.remove_missing(nodes);
        disconnect.0.append(&mut connected.remove_missing(nodes).0);
        let mut connect = vec![];

        // Check if there is space for new nodes. If there is, select a random
        // list of nodes to be added to the connected nodes.
        let mut missing =
            self.cfg.max_connections as i32 - (connected.0.len() + connecting.0.len()) as i32;
        if missing > 0 {
            let mut new_nodes = nodes.clone();
            new_nodes.remove_existing(&connected);
            new_nodes.remove_existing(&connecting);
            if missing as usize > new_nodes.0.len() {
                missing = new_nodes.0.len() as i32;
            }
            connect = new_nodes
                .0
                .choose_multiple(&mut rand::thread_rng(), missing as usize)
                .cloned()
                .collect();
            connecting.0.append(&mut connect.clone());
        }
        self.nodes.update(connecting, connected);
        Message::SetupConnections(NodeIDs(connect), disconnect)
    }

    /// When a requested connection is set up.
    /// returns the list of currently connected nodes
    pub fn new_connection(&mut self, node: &NodeID) -> NodeIDs {
        self.nodes.connect(node);
        // TODO: check if too many nodes are connected now and return the nodes
        // to disconnect - the oldest nodes should disconnect first
        self.nodes.get_connected_nodes()
    }

    /// Returns a clone of the connected NodeIDs.
    pub fn connected(&self) -> NodeIDs {
        self.nodes.get_connected_nodes()
    }

    /// Checks if some of the nodes need to be replaced by other nodes.
    /// The tick itself can be any chosen time-interval.
    pub fn tick(&mut self) -> Message {
        // TODO: check for
        // - connecting nodes with a timeout and disconnect them
        // - connected nodes that are too old and propose new nodes to connect to
        Message::SetupConnections(NodeIDs::new(0), NodeIDs::new(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Error;

    // Test three nodes coming up one after the other, then the first
    // one disappears.
    #[test]
    fn test_new_nodes() -> Result<(), Error> {
        let mut m = Module::new(Some(Config {
            max_connections: 2,
            churn_tick: 2,
        }));

        let nodes = NodeIDs::new(4);

        assert_eq!(
            m.new_nodes(&nodes.slice(0, 1)),
            Message::SetupConnections(nodes.slice(0, 1), NodeIDs::new(0))
        );
        assert_eq!(
            m.new_nodes(&nodes.slice(0, 2)),
            Message::SetupConnections(nodes.slice(1, 1), NodeIDs::new(0))
        );
        assert_eq!(
            m.new_nodes(&nodes.slice(0, 3)),
            Message::SetupConnections(NodeIDs::new(0), NodeIDs::new(0))
        );
        assert_eq!(
            m.new_nodes(&nodes.slice(1, 2)),
            Message::SetupConnections(nodes.slice(2, 1), nodes.slice(0, 1))
        );
        assert_eq!(m.connected(), NodeIDs::new(0));

        Ok(())
    }

    // Test connection of new nodes
    #[test]
    fn test_new_connections() -> Result<(), Error> {
        let mut m = Module::new(Some(Config {
            max_connections: 2,
            churn_tick: 2,
        }));

        let nodes = NodeIDs::new(4);

        m.new_nodes(&nodes.slice(0, 2));
        assert_eq!(m.new_connection(&nodes.0[0]), nodes.slice(0, 1));
        assert_eq!(m.new_connection(&nodes.0[3]), nodes.slice(0, 1));
        assert_eq!(m.connected(), nodes.slice(0, 1));

        Ok(())
    }

    // Make sure that .tick() creates a churn in the nodes.
    #[test]
    fn test_tick() -> Result<(), Error> {
        let mut m = Module::new(Some(Config {
            max_connections: 2,
            churn_tick: 2,
        }));

        Ok(())
    }
}
