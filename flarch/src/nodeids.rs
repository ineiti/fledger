use bytes::Bytes;
use rand::random;
use serde::{Deserialize, Serialize};
use serde_with::{hex::Hex, serde_as};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::num::ParseIntError;
use std::{fmt, str::FromStr};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Give no more than 64 hexadecimal characters")]
    HexTooLong,
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
}

/// Nicely formatted 256 bit structure
#[serde_as]
#[derive(Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default, PartialOrd, Ord)]
pub struct U256(#[serde_as(as = "Hex")] [u8; 32]);

impl fmt::Display for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for index in 0..8 {
            f.write_fmt(format_args!("{:02x}", self.0[index]))?;
        }
        Ok(())
    }
}

impl AsRef<[u8]> for U256 {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, byte) in self.0.iter().enumerate() {
            if index % 8 == 0 && index > 0 {
                f.write_str("-")?;
            }
            f.write_fmt(format_args!("{:02x}", byte))?;
        }
        Ok(())
    }
}

fn to_varint(i: usize) -> Vec<u8> {
    if i < 0x80 {
        return vec![i as u8];
    }
    let mut out = i.to_be_bytes().to_vec();
    out = out.into_iter().skip_while(|&x| x == 0).collect();
    let len = out.len();
    let mut result = vec![len as u8 | 0x80];
    result.extend(out);
    result
}

impl U256 {
    pub fn rnd() -> U256 {
        U256 { 0: random() }
    }

    pub fn hash_data(data: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(to_varint(data.len()));
        hasher.update(data);
        Self(hasher.finalize().into())
    }

    pub fn hash_domain_parts(domain: &str, parts: &[&[u8]]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(to_varint(domain.len()));
        hasher.update(domain);
        hasher.update(to_varint(parts.len()));
        for part in parts {
            hasher.update(to_varint(part.len()));
            hasher.update(part);
        }

        Self(hasher.finalize().into())
    }

    pub fn hash_domain_hashmap<K: Serialize, V: Serialize>(
        domain: &str,
        hm: &HashMap<K, V>,
    ) -> Self {
        let mut parts = vec![];
        for (k, v) in hm {
            parts.push(rmp_serde::to_vec(k).unwrap());
            parts.push(rmp_serde::to_vec(v).unwrap());
        }
        let slices: Vec<&[u8]> = parts.iter().map(|e| e.as_slice()).collect();
        Self::hash_domain_parts(domain, &slices)
    }

    pub fn from_vec<V: Serialize>(domain: &str, vec: &Vec<V>) -> Self {
        let mut parts = vec![];
        for v in vec {
            parts.push(rmp_serde::to_vec(v).unwrap());
        }
        let slices: Vec<&[u8]> = parts.iter().map(|e| e.as_slice()).collect();
        Self::hash_domain_parts(domain, &slices)
    }

    pub fn to_bytes(self) -> [u8; 32] {
        self.0
    }

    pub fn bytes(&self) -> Bytes {
        Bytes::copy_from_slice(self.as_ref())
    }

    pub fn zero() -> U256 {
        U256 { 0: [0; 32] }
    }
}

impl FromStr for U256 {
    type Err = anyhow::Error;

    /// Convert a hexadecimal string to a U256.
    /// If less than 64 characters are given, the U256 is filled from
    /// the left with the remaining u8 initialized to 0.
    /// So
    ///   `U256.from_str("1234") == U256.from_str("123400")`
    /// something
    fn from_str(s: &str) -> anyhow::Result<U256> {
        if s.len() > 64 {
            return Err(ParseError::HexTooLong.into());
        }
        let v: Vec<u8> = (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.into()))
            .collect::<anyhow::Result<Vec<u8>>>()?;
        let mut u = U256 { 0: [0u8; 32] };
        v.iter().enumerate().for_each(|(i, b)| u.0[i] = *b);
        Ok(u)
    }
}

impl From<[u8; 32]> for U256 {
    fn from(b: [u8; 32]) -> Self {
        U256 { 0: b }
    }
}

impl fmt::LowerHex for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0.iter() {
            f.write_fmt(format_args!("{:02x}", byte))?;
        }
        Ok(())
    }
}

/// A node ID for a node, which is a U256.
pub type NodeID = U256;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Default)]
/// A list of node IDs with some useful methods.
pub struct NodeIDs(pub Vec<NodeID>);

impl NodeIDs {
    /// Returns a NodeIDs with 'nbr' random NodeID inside.
    pub fn new(nbr: u32) -> Self {
        let mut ret = NodeIDs { 0: vec![] };
        for _ in 0..nbr {
            ret.0.push(NodeID::rnd())
        }
        ret
    }

    // Returns an empty NodeIDs
    pub fn empty() -> Self {
        NodeIDs::new(0)
    }

    /// Returns a new NodeIDs with a copy of the NodeIDs from..from+len
    pub fn slice(&self, from: usize, len: usize) -> Self {
        NodeIDs {
            0: self.0[from..(from + len)].to_vec(),
        }
    }

    /// Goes through all nodes in the structure and removes the ones that
    /// are NOT in the argument.
    /// It returns the removed nodes.
    pub fn remove_missing(&mut self, nodes: &NodeIDs) -> NodeIDs {
        self.remove(nodes, false)
    }

    /// Goes through all nodes in the structure and removes the ones that
    /// are in the argument.
    /// It returns the removed nodes.
    pub fn remove_existing(&mut self, nodes: &NodeIDs) -> NodeIDs {
        self.remove(nodes, true)
    }

    // Checks whether any of the the nodes in the two NodeIDs match.
    // Returns 'true' if at least one match is found.
    // Returns 'false' if no matches are found.
    pub fn contains_any(&self, other: &NodeIDs) -> bool {
        for node in &self.0 {
            for node_other in &other.0 {
                if node == node_other {
                    return true;
                }
            }
        }
        false
    }

    // Checks whether all of the the nodes in 'other' can be found.
    // Returns 'true' if all nodes are found found.
    // Returns 'false' if one or more nodes are missing.
    pub fn contains_all(&self, other: &NodeIDs) -> bool {
        for node_other in &other.0 {
            if !self.0.contains(node_other) {
                return false;
            }
        }
        true
    }

    /// Merges all other nodes into this list. Ignores nodes already in this list.
    pub fn merge(&mut self, other: NodeIDs) {
        self.0.extend(
            other
                .0
                .into_iter()
                .filter(|n| !self.0.contains(n))
                .collect::<Vec<NodeID>>(),
        );
    }

    fn remove(&mut self, nodes: &NodeIDs, exists: bool) -> NodeIDs {
        let mut ret = vec![];
        let mut i = 0;
        while i < self.0.len() {
            if nodes.0.contains(&self.0[i]) == exists {
                ret.push(self.0.remove(i));
            } else {
                i += 1;
            }
        }
        NodeIDs(ret)
    }
}

impl From<Vec<U256>> for NodeIDs {
    fn from(nodes: Vec<U256>) -> Self {
        Self { 0: nodes }
    }
}

impl From<&Vec<U256>> for NodeIDs {
    fn from(nodes: &Vec<U256>) -> Self {
        Self { 0: nodes.to_vec() }
    }
}

impl fmt::Display for NodeIDs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "[{}]",
            &self
                .0
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<String>>()
                .join(","),
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::start_logging;

    use super::*;

    #[test]
    fn test_new_nodes() {
        let mut nodes = NodeIDs::new(4);

        let nodes2 = nodes.remove_missing(&NodeIDs::new(0));
        assert_eq!(0, nodes.0.len());
        assert_eq!(4, nodes2.0.len());
    }

    #[test]
    fn test_serialize() -> anyhow::Result<()> {
        start_logging();

        let id = U256::rnd();

        let id_yaml = serde_yaml::to_string(&id)?;
        log::info!("yaml is: {id_yaml}");
        let id_clone: U256 = serde_yaml::from_str(&id_yaml)?;

        assert_eq!(id, id_clone);

        Ok(())
    }
}
