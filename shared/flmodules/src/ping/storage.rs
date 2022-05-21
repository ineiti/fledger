use std::collections::HashMap;

use crate::nodeids::NodeID;
use serde::{Deserialize, Serialize};

use super::module::PingConfig;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PingStorage {
    pub stats: HashMap<NodeID, PingStat>,
    pub ping: Vec<NodeID>,
    pub failed: Vec<NodeID>,
    config: PingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PingStat {
    pub countdown: i32,
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
                countdown: 0,
                rx: 0,
                tx: 0,
            });
        self.ping.push(id);
    }

    pub fn pong(&mut self, id: NodeID) {
        if let Some(mut stat) = self.stats.remove(&id){
            stat.countdown = self.config.interval as i32;
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
            stat.countdown -= 1;
            if stat.countdown + self.config.timeout as i32 <= 0  {
                failed.push(*id);
            } else if stat.countdown <= 0 {
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
            interval: 3,
            timeout: 2,
        });
        let n1 = NodeID::rnd();
        let n2 = NodeID::rnd();

        s.new_node(n1);
        assert_eq!(1, s.stats.len());
        assert_eq!(2, s.stats.get(&n1).unwrap().countdown);
        assert_eq!(vec![n1], s.ping);
        s.tick();
        assert_eq!(1, s.stats.get(&n1).unwrap().countdown);
        s.tick();
        assert_eq!(0, s.stats.len());
        assert_eq!(1, s.failed.len());

        s.new_node(n1);
        s.pong(n1);
        assert_eq!(1, s.stats.len());
        assert_eq!(5, s.stats.get(&n1).unwrap().countdown);
        s.tick();
        s.tick();

        s.new_node(n2);
        assert_eq!(2, s.stats.len());
        s.tick();
        assert_eq!(2, s.stats.len());
    }
}
