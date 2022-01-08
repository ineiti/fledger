use super::nodeids::NodeID;
use super::nodeids::NodeIDs;
use rand::seq::SliceRandom;

mod nodes;
use nodes::*;

/// RandomConnections listens for new available nodes and then chooses
/// to randomly connect to a set number of nodes.
#[derive(Debug)]
pub struct Module {
    cfg: Config,
    nodes_connected: Nodes,
    nodes_connecting: Nodes,
    all_nodes: NodeIDs,
}

/// Message is a list of nodes to connect to,
/// and a list of nodes to disconnect from.
pub type Message = (NodeIDs, NodeIDs);

#[derive(Debug)]
pub struct Config {
    /// How many maximum connections the system tries to make
    pub max_connections: u32,

    /// How many ticks a node stays in the list before it is
    /// possibly replaced by another node.
    pub churn_connected: u32,

    /// How long a node stays in the connecting-queue
    pub connecting_timeout: u32,
}

impl Config {
    pub fn default() -> Self {
        Config {
            max_connections: 2,
            churn_connected: 60,
            connecting_timeout: 1,
        }
    }
}

impl Module {
    pub fn new(cfg: Option<Config>) -> Self {
        Module {
            cfg: cfg.unwrap_or(Config::default()),
            nodes_connected: Nodes::new(),
            nodes_connecting: Nodes::new(),
            all_nodes: NodeIDs::empty(),
        }
    }

    /// When a new list of nodes is available.
    /// returns the list of nodes to (dis)connect to.
    pub fn new_nodes(&mut self, nodes: &NodeIDs) -> Message {
        self.all_nodes = nodes.clone();
        self.update_nodes()
    }

    fn update_nodes(&mut self) -> Message {
        let nodes = &self.all_nodes;

        // Search for nodes that are not available anymore
        // and that should be disconnected.
        let mut disconnect = self.nodes_connecting.remove_missing(nodes);
        disconnect
            .0
            .append(&mut self.nodes_connected.remove_missing(nodes).0);

        // Check if there is space for new nodes. If there is, select a random
        // list of nodes to be added to the connected nodes.
        let churn = self
            .nodes_connected
            .oldest_ticks(self.cfg.churn_connected)
            .0
            .len();
        let missing = self.cfg.max_connections as i32
            - (self.nodes_connected.0.len() + self.nodes_connecting.0.len() - churn) as i32;
        if missing > 0 {
            let connect = self.connect_nodes(missing);
            self.nodes_connecting.add_new(connect.0.clone());
            (connect, disconnect)
        } else {
            (NodeIDs::empty(), disconnect)
        }
    }

    fn connect_nodes(&self, mut nbr: i32) -> NodeIDs {
        let mut new_nodes = self.all_nodes.clone();
        new_nodes.remove_existing(&self.nodes_connected.get_nodes());
        new_nodes.remove_existing(&self.nodes_connecting.get_nodes());
        if nbr > new_nodes.0.len() as i32 {
            nbr = new_nodes.0.len() as i32;
        }
        NodeIDs {
            0: new_nodes
                .0
                .choose_multiple(&mut rand::thread_rng(), nbr as usize)
                .cloned()
                .collect(),
        }
    }

    /// When a requested connection is set up.
    /// returns the list of currently connected nodes
    pub fn new_connection(&mut self, node: &NodeID) -> Message {
        if self.nodes_connecting.contains(node) {
            self.nodes_connecting.0.retain(|nt| nt.id != *node);
            self.nodes_connected.add_new(vec![node.clone()]);
        }

        let mut disconnect = NodeIDs::empty();
        let too_many = self.nodes_connected.0.len() as i64 - self.cfg.max_connections as i64;
        if too_many > 0 {
            disconnect = self.nodes_connected.remove_oldest_n(too_many as usize);
        }
        (NodeIDs::empty(), disconnect)
    }

    /// Returns a clone of the connected NodeIDs.
    pub fn connected(&self) -> NodeIDs {
        self.nodes_connected.get_nodes()
    }

    /// Checks if some of the nodes need to be replaced by other nodes.
    /// The tick itself can be any chosen time-interval.
    pub fn tick(&mut self) -> Message {
        self.nodes_connected.tick();
        self.nodes_connecting.tick();

        let mut disconnect = self
            .nodes_connecting
            .remove_oldest_ticks(self.cfg.connecting_timeout);

        let (connect, mut disc) = self.update_nodes();
        disconnect.0.append(&mut disc.0);
        (connect, disconnect)
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
            churn_connected: 2,
            connecting_timeout: 2,
        }));

        let nodes = NodeIDs::new(4);

        assert_eq!(
            m.new_nodes(&nodes.slice(0, 1)),
            (nodes.slice(0, 1), NodeIDs::empty())
        );
        assert_eq!(
            m.new_nodes(&nodes.slice(0, 2)),
            (nodes.slice(1, 1), NodeIDs::empty())
        );
        assert_eq!(
            m.new_nodes(&nodes.slice(0, 3)),
            (NodeIDs::empty(), NodeIDs::empty())
        );
        assert_eq!(
            m.new_nodes(&nodes.slice(1, 2)),
            (nodes.slice(2, 1), nodes.slice(0, 1))
        );
        assert_eq!(m.connected(), NodeIDs::empty());

        Ok(())
    }

    // Test connection of new nodes
    #[test]
    fn test_new_connections() -> Result<(), Error> {
        let mut m = Module::new(Some(Config {
            max_connections: 2,
            churn_connected: 2,
            connecting_timeout: 2,
        }));

        let nodes = NodeIDs::new(4);

        let (conn, _) = assert_msg_len(&m.new_nodes(&nodes.slice(0, 2)), 2, 0);
        assert_msg_len(&m.new_connection(&conn.0[0]), 0, 0);
        assert_eq!(m.connected(), conn.slice(0, 1));
        assert_msg_len(&m.new_connection(&conn.0[0]), 0, 0);
        assert_eq!(m.connected(), nodes.slice(0, 1));
        assert_msg_len(&m.new_connection(&conn.0[1]), 0, 0);
        assert_eq!(m.connected(), nodes.slice(0, 2));

        Ok(())
    }

    fn assert_msg_len(msg: &Message, len_conn: usize, len_disconn: usize) -> (NodeIDs, NodeIDs) {
        let (conn, disc) = msg;
        assert_eq!(
            len_conn,
            conn.0.len(),
            "Wrong number of connections: {} instead of {}",
            conn.0.len(),
            len_conn
        );
        assert_eq!(
            len_disconn,
            disc.0.len(),
            "Wrong number of disconnections: {} instead of {}",
            disc.0.len(),
            len_disconn
        );
        (conn.clone(), disc.clone())
    }

    // Make sure that .tick() creates a churn in the nodes.
    #[test]
    fn test_tick() -> Result<(), Error> {
        let mut m = Module::new(Some(Config {
            max_connections: 2,
            churn_connected: 2,
            connecting_timeout: 2,
        }));

        let n = NodeIDs::new(5);

        // Connect two nodes out of 4.
        let msg = m.new_nodes(&n.slice(0, 4));
        let (conn, _) = assert_msg_len(&msg, 2, 0);
        assert_msg_len(&m.new_connection(&conn.0[0]), 0, 0);
        assert_msg_len(&m.new_connection(&conn.0[1]), 0, 0);
        assert_eq!(2, m.connected().0.len());

        // Churn through the nodes.
        assert_msg_len(&m.tick(), 0, 0);
        let (conn2, _) = assert_msg_len(&m.tick(), 2, 0);
        assert!(!conn.contains_any(&conn2));
        assert_eq!(2, m.connected().0.len());
        m.new_connection(&conn2.0[0]);
        m.new_connection(&conn2.0[1]);
        assert!(m.connected() == conn2);

        Ok(())
    }

    fn make_nodes(n: usize) -> Nodes {
        let mut nodes = Nodes::new();
        nodes.add_new(NodeIDs::new(n as u32).0);
        for node in 0..n {
            nodes.0[node].ticks = node as u32 + 1;
        }
        nodes
    }

    // Tests the nodes and the remove methods
    #[test]
    fn test_nodes_remove() -> Result<(), Error> {
        let mut nodes = make_nodes(5);
        let mut removed = nodes.remove_oldest_n(2);
        assert_eq!(removed.0.len(), 2);
        assert_eq!(nodes.0.len(), 3);

        removed = nodes.remove_oldest_n(5);
        assert_eq!(removed.0.len(), 3);
        assert_eq!(nodes.0.len(), 0);

        nodes = make_nodes(5);
        removed = nodes.remove_oldest_ticks(6);
        assert_eq!(nodes.0.len(), 5);
        assert_eq!(removed.0.len(), 0);

        removed = nodes.remove_oldest_ticks(4);
        assert_eq!(nodes.0.len(), 3);
        assert_eq!(removed.0.len(), 2);

        Ok(())
    }
}
