use serde::{Deserialize, Serialize};
use sha2::digest::{consts::U32, generic_array::GenericArray};
use std::fmt;
use rand::random;
use thiserror::Error;
use std::num::ParseIntError;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Give no more than 64 hexadecimal characters")]
    HexTooLong,
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
}

/// Nicely formatted 256 bit structure
#[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash)]
pub struct U256([u8; 32]);

impl fmt::Display for U256 {
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

impl AsRef<[u8]> for U256 {
    fn as_ref(&self) -> &[u8] {
        return &self.0;
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

impl U256 {
    pub fn rnd() -> U256 {
        U256 { 0: random() }
    }

    /// Convert a hexadecimal string to a U256.
    /// If less than 64 characters are given, the U256 is filled from
    /// the left with the remaining u8 initialized to 0.
    /// So
    ///   `U256.from_str("1234") == U256.from_str("123400")`
    /// something
    pub fn from_str(s: &str) -> Result<U256, ParseError> {
        if s.len() > 64 {
            return Err(ParseError::HexTooLong);
        }
        let v: Vec<u8> = (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
            .collect::<Result<Vec<u8>, ParseIntError>>()?;
        let mut u = U256 { 0: [0u8; 32] };
        v.iter().enumerate().for_each(|(i, b)| u.0[i] = *b);
        Ok(u)
    }

    pub fn to_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl From<GenericArray<u8, U32>> for U256 {
    fn from(ga: GenericArray<u8, U32>) -> Self {
        let mut u = U256 { 0: [0u8; 32] };
        ga.as_slice()
            .iter()
            .enumerate()
            .for_each(|(i, b)| u.0[i] = *b);
        u
    }
}

impl From<[u8; 32]> for U256 {
    fn from(b: [u8; 32]) -> Self {
        U256 { 0: b }
    }
}

pub type NodeID = U256;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
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
        return false;
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
        Self { 0: nodes.iter().cloned().collect() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Error;

    #[test]
    fn test_new_nodes() -> Result<(), Error> {
        let mut nodes = NodeIDs::new(4);

        let nodes2 = nodes.remove_missing(&NodeIDs::new(0));
        assert_eq!(0, nodes.0.len());
        assert_eq!(4, nodes2.0.len());

        Ok(())
    }
}
