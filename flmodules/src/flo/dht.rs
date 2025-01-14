use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::crypto::access::History;

use super::flo::{Flo, FloData, FloError, FloID, FloWrapper};

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
    flo: Flo,
    spread: u32,
    // TODO: Add the domain of this flo here.
}

impl DHTFlo {
    pub fn from_wrapper<T: DeserializeOwned + Serialize + Clone>(
        wrapper: FloWrapper<T>,
        spread: u32,
    ) -> Self {
        Self {
            flo: wrapper.flo(),
            spread,
        }
    }

    pub fn id(&self) -> FloID {
        self.flo.id.clone()
    }

    pub fn content(&self) -> String {
        self.flo.content.clone()
    }

    pub fn current(&self) -> FloData {
        self.flo.current.clone()
    }

    pub fn history(&self) -> History<FloData> {
        self.flo.history.clone()
    }

    pub fn version(&self) -> usize {
        self.flo.version()
    }

    pub fn spread(&self) -> u32 {
        self.spread
    }
}

impl<T: Serialize + DeserializeOwned + Clone> TryFrom<DHTFlo> for FloWrapper<T> {
    type Error = FloError;

    fn try_from(value: DHTFlo) -> Result<Self, Self::Error> {
        value.flo.try_into()
    }
}
