use std::collections::HashMap;

use flarch::nodeids::NodeID;
use serde::{Deserialize, Serialize};

use super::messages::PingConfig;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PingStorage {
    pub stats: HashMap<NodeID, PingStat>,
    pub ping: Vec<NodeID>,
    pub failed: Vec<NodeID>,
    config: PingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PingStat {
    pub lastping: u32,
    pub rx: u32,
    pub tx: u32,
}

impl PingStorage {
    pub fn new(config: PingConfig) -> Self {
        Self {
            stats: HashMap::new(),
            ping: vec![],
            failed: vec![],
            config,
        }
    }

    pub fn new_node(&mut self, id: NodeID) {
        if self.stats.contains_key(&id) {
            return;
        }
        self.stats
            .insert(id, PingStat{
                lastping: 0,
                rx: 0,
                tx: 1,
            });
        self.ping.push(id);
    }

    pub fn pong(&mut self, id: NodeID) {
        if let Some(mut stat) = self.stats.remove(&id){
            stat.lastping = 0;
            stat.rx += 1;
            self.stats.insert(id, stat);
        } else {
            self.new_node(id);
        }
    }

    pub fn tick(&mut self) {
        self.ping.clear();
        self.failed.clear();
        self.tick_countdown();
    }

    fn tick_countdown(&mut self) {
        let mut failed = vec![];
        for (id, stat) in self.stats.iter_mut() {
            stat.lastping += 1;
            if stat.lastping >= self.config.timeout + self.config.interval {
                failed.push(*id);
            } else if stat.lastping >= self.config.interval {
                stat.tx += 1;
                self.ping.push(*id);
            }
        }
        for id in failed {
            self.stats.remove(&id);
            self.failed.push(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ticks() {
        let mut s = PingStorage::new(PingConfig {
            interval: 1,
            timeout: 2,
        });
        let n1 = NodeID::rnd();
        let n2 = NodeID::rnd();

        s.new_node(n1);
        assert_eq!(1, s.stats.len());
        assert_eq!(0, s.stats.get(&n1).unwrap().lastping);
        assert_eq!(vec![n1], s.ping);
        s.tick();
        assert_eq!(1, s.stats.get(&n1).unwrap().lastping);
        s.tick();
        assert_eq!(2, s.stats.get(&n1).unwrap().lastping);
        s.tick();
        assert_eq!(0, s.stats.len());
        assert_eq!(1, s.failed.len());

        s.new_node(n1);
        s.pong(n1);
        assert_eq!(1, s.stats.len());
        assert_eq!(0, s.stats.get(&n1).unwrap().lastping);
        s.tick();
        s.tick();

        s.new_node(n2);
        assert_eq!(2, s.stats.len());
        s.tick();
        assert_eq!(1, s.stats.len());
        assert_eq!(1, s.failed.len());
    }
}
