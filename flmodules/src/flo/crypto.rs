use flcrypto::{
    access::{BadgeLink, ConditionLink},
    signer::Verifier,
};
use serde::{Deserialize, Serialize};

use super::flo::FloWrapper;

/// A Verifier is a public key which can verify a signature from a private key.
pub type FloVerifier = FloWrapper<Verifier>;

/// A badge wraps a Condition with a fixed ID.
pub type FloBadge = FloWrapper<BadgeCond>;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BadgeCond(ConditionLink);

impl BadgeCond {
    pub fn new(cond: ConditionLink) -> Self {
        Self(cond)
    }

    pub fn cond(&self) -> &ConditionLink {
        &self.0
    }
}

impl FloBadge {
    pub fn badge_link(&self) -> BadgeLink {
        BadgeLink {
            id: (*self.flo_id()).into(),
            version: self.version(),
            condition: self.cache().0.clone(),
        }
    }
}
