/// A simple stats structure that keeps track of which pings it
/// received.
/// TODO: update with OKs received from pings, to be sure that the
/// round-trip works, and that the rx- and tx-pings are actually correlated.
use std::sync::mpsc::Sender;

use super::LOutput;
use crate::{
    node::{
        config::{NodeConfig, NodeInfo},
        network::connection_state::CSEnum,
    },
    signal::web_rtc::{ConnectionStateMap, NodeStat, WebRTCConnectionState},
    types::U256,
};

pub mod statnode;
use statnode::{ConnState, StatNodes};

pub struct Stats {
    pub stats: StatNodes,
    pub node_config: NodeConfig,
    pub output_tx: Sender<LOutput>,
}

impl Stats {
    pub fn new(node_config: NodeConfig, output_tx: Sender<LOutput>) -> Stats {
        return Stats {
            stats: StatNodes::new(),
            node_config,
            output_tx,
        };
    }

    pub fn send_stats(&mut self) -> Result<(), String> {
        // Send statistics to the signalling server
        if self.stats.tick(self.node_config.send_stats) {
            self.stats.expire(self.node_config.stats_ignore);

            let stats: Vec<NodeStat> = self.stats.collect(&&self.node_config.our_node.get_id());
            self.output_tx
                .send(LOutput::SendStats(stats))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn update_connection_state(
        &mut self,
        id: U256,
        dir: WebRTCConnectionState,
        st: CSEnum,
        state: Option<ConnectionStateMap>,
    ) {
        self.stats
            .upsert(&id, dir, ConnState::from_states(st, state));
    }

    pub fn store_nodes(&mut self, nodes: Vec<NodeInfo>) {
        for ni in nodes {
            self.stats.update(ni);
        }
    }

    pub fn ping_all(&mut self) -> Result<(), String> {
        self.stats
            .ping_all(&&self.node_config.our_node.get_id(), self.output_tx.clone())
    }

    pub fn ping_rcv(&mut self, from: &U256) {
        self.stats.ping_rcv(&from);
    }
}

#[cfg(test)]
mod tests {
    use super::Stats;
    use crate::node::{
        config::{NodeConfig},
        logic::LOutput,
    };
    use log::LevelFilter;
    use std::{sync::mpsc::channel, thread::sleep, time::Duration};

    #[test]
    /// Starts a Logic with two nodes, then receives ping only from one node.
    /// The other node should be removed from the stats-list.
    fn cleanup_stale_nodes() -> Result<(), String> {
        simple_logging::log_to_stderr(LevelFilter::Trace);

        let nc1 = NodeConfig::new();
        let n1 = nc1.our_node;
        let nc2 = NodeConfig::new();
        let n2 = nc2.our_node;
        let mut nc = NodeConfig::new();
        nc.send_stats = 1f64;
        nc.stats_ignore = 2f64;
        let (tx, _rx) = channel::<LOutput>();

        let mut ns = Stats::new(nc.clone(), tx.clone());

        ns.store_nodes(vec![n1.clone(), n2.clone()]);
        assert_eq!(2, ns.stats.len(), "Should have two nodes now");

        sleep(Duration::from_millis(10));
        ns.ping_rcv(&n1.get_id());
        ns.send_stats()?;
        assert_eq!(1, ns.stats.len(), "One node should disappear");
        Ok(())
    }
}
