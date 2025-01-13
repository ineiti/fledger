use serde::{Deserialize, Serialize};

use super::flo::{Flo, FloID, FloWrapper};

pub type DHTConfig = FloWrapper<DHTConfigData>;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DHTConfigData {
    // Put the DHTFlos in round(DHTFlo.spread * over_provide) nodes
    pub over_provide: f32,
    // 16 exa bytes should be enough for everybody
    pub max_space: usize,
}

impl DHTConfig {}

/// A DHTFlo is a storage unit to be stored in a node. It holds any type of
/// Flo in it.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DHTFlo {
    pub flo: Flo,
    pub spread: u32,
    // TODO: Add the domain of this flo here.
}

impl DHTFlo {
    pub fn id(&self) -> FloID {
        self.flo.id.clone()
    }
}
