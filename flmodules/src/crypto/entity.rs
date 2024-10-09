use flarch::nodeids::U256;
use serde::{Deserialize, Serialize};

use super::{group::GroupVerifier, signer::Verifier};

/**
 *
 */

#[derive(Serialize, Deserialize, Clone)]
pub enum Entity {
    Single(Verifier),
    Group(GroupVerifier),
}

impl Entity {
    pub fn new_single(ver: Verifier) -> Self {
        Self::Single(ver)
    }

    pub fn new_group(entities: Vec<Self>, threshold: usize) -> Self {
        Self::Group(GroupVerifier::new(entities, threshold))
    }

    pub fn get_id(&self) -> EntityID {
        match self {
            Self::Single(single) => single.get_id(),
            Self::Group(group) => group.get_id(),
        }
    }
}

pub type EntityID = U256;
