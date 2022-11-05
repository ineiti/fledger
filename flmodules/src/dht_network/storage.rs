use std::{cmp::min, collections::HashMap, fmt::Debug};

use crate::nodeids::NodeID;

pub struct Kademlia {
    k_buckets: HashMap<usize, Vec<Box<dyn ID>>>,
    k: usize,
    base: Box<dyn ID>,
}

impl Kademlia {
    pub fn new(k: usize, base: Box<dyn ID>) -> Self {
        Self {
            k_buckets: HashMap::new(),
            k,
            base,
        }
    }

    pub fn add_nodes(&mut self, nodes: Vec<Box<dyn ID>>) {
        for node in nodes {
            if let Some(bucket) = self.k_buckets.get_mut(&self.base.distance(&*node)) {
                bucket.push(node);
                if bucket.len() > self.k {
                    bucket.sort_by(|a, b| a.uptime_sec().cmp(&b.uptime_sec()));
                    bucket.drain(self.k..);
                }
            } else {
                self.k_buckets
                    .insert(self.base.distance(&*node), vec![node]);
            }
        }
    }

    pub fn remove_nodes(&mut self, nodes: Vec<Box<dyn ID>>) {
        for node in nodes {
            if let Some(bucket) = self.k_buckets.get_mut(&self.base.distance(&*node)) {
                bucket.retain(|n| !node.equals(n.as_ref()));
            }
        }
    }

    pub fn closest_nodes(&mut self, node: &dyn ID, nbr: usize) -> Vec<Box<dyn ID>> {
        let dist = self.base.distance(node);
        let mut closest = vec![];
        // Zigzag from the bucket 'dist' back and forth and fill 'closest' with the
        // nodes found.
        for i in 0..self.base.bits() {
            let mut zigzag = dist + i / 2;
            if i % 2 == 1 {
                if zigzag > i {
                    zigzag -= i;
                } else {
                    continue;
                }
            }
            if let Some(bucket) = self.k_buckets.get_mut(&zigzag) {
                let elements = min(nbr - closest.len(), bucket.len());
                closest.extend(
                    bucket
                        .drain(0..elements)
                        .map(|n| n)
                        .collect::<Vec<Box<dyn ID>>>(),
                );
            }
            if closest.len() == nbr {
                break;
            }
        }

        closest
    }
}

pub trait ID: Debug {
    fn uptime_sec(&self) -> usize;

    fn get_bytes(&self) -> Vec<u8>;

    fn bits(&self) -> usize {
        self.get_bytes().len() * 8
    }

    fn distance(&self, other: &dyn ID) -> usize {
        let xor = self.get_bytes();
        let mut xor_other = other.get_bytes();
        xor.iter()
            .map(|c| (c ^ xor_other.remove(0)).count_ones())
            .reduce(|a, i| a + i)
            .unwrap() as usize
    }

    fn equals(&self, other: &dyn ID) -> bool {
        let mut other_bytes = other.get_bytes().to_vec();
        self.get_bytes()
            .to_vec()
            .iter()
            .all(|b| b == &other_bytes.remove(0))
    }
}

impl ID for NodeID {
    fn get_bytes(&self) -> Vec<u8> {
        self.to_bytes().to_vec()
    }

    fn uptime_sec(&self) -> usize {
        self.get_bytes()[0] as usize
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use flarch::start_logging;

    use super::*;

    #[test]
    fn test_id() {
        start_logging();
        
        assert_eq!(256, NodeID::rnd().bits());
        let id1 = NodeID::from_str("10").unwrap();
        assert!(id1.equals(&id1));
        let id2 = NodeID::from_str("20").unwrap();
        assert!(!id1.equals(&id2));
        log::info!("Distance is: {}", id1.distance(&id2));
        assert_eq!(2, id1.distance(&id2));
        assert_eq!(2, id2.distance(&id1));
    }

    #[test]
    fn add_remove_nodes() {
        start_logging();

        let mut ids: Vec<Box<dyn ID>> = vec![];
        for i in 0..8 {
            ids.push(Box::new(NodeID::from_str(&format!("0{i}")).unwrap()));
        }

        let mut km = Kademlia::new(2, ids.remove(4));
        km.add_nodes(ids);
        for (dist, bucket) in km.k_buckets{
            log::info!("Distance {dist}: {bucket:?}");
            assert!(bucket.len() <= 2);
        }
    }
}
