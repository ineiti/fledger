mod nodes;
use nodes::*;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use types::nodeids::NodeID;
use types::nodeids::NodeIDs;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageIn {
    NodeList(NodeIDs),
    NodeConnected(NodeID),
    NodeDisconnected(NodeID),
    Tick,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageOut {
    ConnectNode(NodeID),
    DisconnectNode(NodeID),
    ListUpdate(NodeIDs),
}

/// RandomConnections listens for new available nodes and then chooses
/// to randomly connect to a set number of nodes.
#[derive(Debug)]
pub struct Module {
    cfg: Config,
    nodes_connected: Nodes,
    nodes_connecting: Nodes,
    all_nodes: NodeIDs,
}

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
            max_connections: 20,
            churn_connected: 60,
            connecting_timeout: 1,
        }
    }
}

impl Module {
    pub fn new(cfg: Config) -> Self {
        Module {
            cfg,
            nodes_connected: Nodes::new(),
            nodes_connecting: Nodes::new(),
            all_nodes: NodeIDs::empty(),
        }
    }

    /// Processes one message and returns messages that need to be treated by the
    /// system.
    pub fn process_message(&mut self, msg: MessageIn) -> Vec<MessageOut> {
        match msg {
            MessageIn::NodeList(nodes) => self.new_nodes(&nodes),
            MessageIn::NodeConnected(node) => self.new_connection(&node),
            MessageIn::NodeDisconnected(node) => self.new_disconnection(&node),
            MessageIn::Tick => self.tick(),
        }
    }

    /// Returns a clone of the connected NodeIDs.
    pub fn connected(&self) -> NodeIDs {
        self.nodes_connected.get_nodes()
    }

    /// When a new list of nodes is available.
    /// returns the list of nodes to (dis)connect to.
    pub fn new_nodes(&mut self, nodes: &NodeIDs) -> Vec<MessageOut> {
        self.all_nodes = nodes.clone();
        self.update_nodes()
    }

    /// When a requested connection is set up.
    /// returns the list of currently connected nodes
    pub fn new_connection(&mut self, node: &NodeID) -> Vec<MessageOut> {
        if self.nodes_connecting.contains(node) {
            self.nodes_connecting.0.retain(|nt| nt.id != *node);
            self.nodes_connected.add_new(vec![node.clone()]);
        }

        let mut disconnect = NodeIDs::empty();
        let too_many = self.nodes_connected.0.len() as i64 - self.nodes_needed();
        if too_many > 0 {
            disconnect = self.nodes_connected.remove_oldest_n(too_many as usize);
        }
        Self::make_connect_disconnect(None, Some(disconnect))
    }

    /// Removes a node that has been disconnected from the list of connected
    /// nodes and disconnecting nodes.
    pub fn new_disconnection(&mut self, node: &NodeID) -> Vec<MessageOut> {
        if self.nodes_connected.contains(node){
            self.nodes_connected.0.retain(|nt| nt.id != *node);
        }
        if self.nodes_connecting.contains(node){
            self.nodes_connecting.0.retain(|nt| nt.id != *node);
        }
        self.update_nodes()
    }

    /// Checks if some of the nodes need to be replaced by other nodes.
    /// The tick itself can be any chosen time-interval.
    pub fn tick(&mut self) -> Vec<MessageOut> {
        self.nodes_connected.tick();
        self.nodes_connecting.tick();

        let disconnect = self
            .nodes_connecting
            .remove_oldest_ticks(self.cfg.connecting_timeout);

        let mut out = self.update_nodes();
        out.extend(Self::make_connect_disconnect(None, Some(disconnect)));
        out
    }

    fn update_nodes(&mut self) -> Vec<MessageOut> {
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
        let missing = self.nodes_needed()
            - (self.nodes_connected.0.len() + self.nodes_connecting.0.len() - churn) as i64;
        if missing > 0 {
            let connect = self.connect_nodes(missing);
            self.nodes_connecting.add_new(connect.0.clone());
            Self::make_connect_disconnect(Some(connect), Some(disconnect))
        } else {
            Self::make_connect_disconnect(None, Some(disconnect))
        }
    }

    fn make_connect_disconnect(
        connect: Option<NodeIDs>,
        disconnect: Option<NodeIDs>,
    ) -> Vec<MessageOut> {
        connect
            .unwrap_or(NodeIDs::empty())
            .0
            .iter()
            .map(|node| MessageOut::ConnectNode(*node))
            .chain(
                disconnect
                    .unwrap_or(NodeIDs::empty())
                    .0
                    .iter()
                    .map(|node| MessageOut::DisconnectNode(*node)),
            )
            .collect()
    }

    fn connect_nodes(&self, mut nbr: i64) -> NodeIDs {
        let mut new_nodes = self.all_nodes.clone();
        new_nodes.remove_existing(&self.nodes_connected.get_nodes());
        new_nodes.remove_existing(&self.nodes_connecting.get_nodes());
        if nbr > new_nodes.0.len() as i64 {
            nbr = new_nodes.0.len() as i64;
        }
        NodeIDs {
            0: new_nodes
                .0
                .choose_multiple(&mut rand::thread_rng(), nbr as usize)
                .cloned()
                .collect(),
        }
    }

    /// Returns the number of nodes needed to have a high probability of a
    /// fully connected network.
    fn nodes_needed(&self) -> i64 {
        let nodes = self.all_nodes.0.len();
        if nodes <= 1 {
            0
        } else {
            ((nodes as f64).ln() * 2.)
                .ceil()
                .min(self.cfg.max_connections as f64) as i64
        }
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
        let mut m = Module::new(Config {
            max_connections: 2,
            churn_connected: 2,
            connecting_timeout: 2,
        });

        let nodes = NodeIDs::new(4);
        let empty = NodeIDs::empty();

        assert_cd(m.new_nodes(&nodes.slice(0, 1)), &nodes.slice(0, 1), &empty);
        assert_cd(m.new_nodes(&nodes.slice(0, 2)), &nodes.slice(1, 1), &empty);
        assert_cd(m.new_nodes(&nodes.slice(0, 3)), &empty, &empty);
        assert_cd(
            m.new_nodes(&nodes.slice(1, 2)),
            &nodes.slice(2, 1),
            &nodes.slice(0, 1),
        );
        assert_eq!(m.connected(), NodeIDs::empty());

        Ok(())
    }

    fn assert_cd(should: Vec<MessageOut>, conn: &NodeIDs, disc: &NodeIDs) {
        let mut should_conn = vec![];
        let mut should_disc = vec![];
        for msg in should {
            match msg {
                MessageOut::ConnectNode(node) => should_conn.push(node),
                MessageOut::DisconnectNode(node) => should_disc.push(node),
            }
        }
        assert_eq!(should_conn, conn.clone());
        assert_eq!(should_disc, disc.clone());
    }

    // Test connection of new nodes
    #[test]
    fn test_new_connections() -> Result<(), Error> {
        let mut m = Module::new(Config {
            max_connections: 2,
            churn_connected: 2,
            connecting_timeout: 2,
        });

        let nodes = NodeIDs::new(4);

        let (conn, _) = assert_msg_len(m.new_nodes(&nodes.slice(0, 2)), 2, 0);
        assert_msg_len(m.new_connection(&conn.0[0]), 0, 0);
        assert_eq!(m.connected(), conn.slice(0, 1));
        assert_msg_len(m.new_connection(&conn.0[0]), 0, 0);
        assert_eq!(m.connected(), nodes.slice(0, 1));
        assert_msg_len(m.new_connection(&conn.0[1]), 0, 0);
        assert_eq!(m.connected(), nodes.slice(0, 2));

        Ok(())
    }

    fn assert_msg_len(
        msgs: Vec<MessageOut>,
        len_conn: usize,
        len_disconn: usize,
    ) -> (NodeIDs, NodeIDs) {
        let (mut conn, mut disc) = (vec![], vec![]);
        for msg in msgs {
            match msg {
                MessageOut::ConnectNode(node) => conn.push(node),
                MessageOut::DisconnectNode(node) => disc.push(node),
            }
        }
        assert_eq!(
            len_conn,
            conn.len(),
            "Wrong number of connections: {} instead of {}",
            conn.len(),
            len_conn
        );
        assert_eq!(
            len_disconn,
            disc.len(),
            "Wrong number of disconnections: {} instead of {}",
            disc.len(),
            len_disconn
        );
        (conn.into(), disc.into())
    }

    // Make sure that .tick() creates a churn in the nodes.
    #[test]
    fn test_tick() -> Result<(), Error> {
        let mut m = Module::new(Config {
            max_connections: 2,
            churn_connected: 2,
            connecting_timeout: 2,
        });

        let n = NodeIDs::new(5);

        // Connect two nodes out of 4.
        let msg = m.new_nodes(&n.slice(0, 4));
        let (conn, _) = assert_msg_len(msg, 2, 0);
        assert_msg_len(m.new_connection(&conn.0[0]), 0, 0);
        assert_msg_len(m.new_connection(&conn.0[1]), 0, 0);
        assert_eq!(2, m.connected().0.len());

        // Churn through the nodes.
        assert_msg_len(m.tick(), 0, 0);
        let (conn2, _) = assert_msg_len(m.tick(), 2, 0);
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
