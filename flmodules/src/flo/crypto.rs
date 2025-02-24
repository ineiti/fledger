use std::collections::HashMap;

use flarch::nodeids::U256;
use flcrypto::{
    access::{Badge, BadgeID, Condition, Version},
    signer::Verifier,
};
use flmacro::AsU256;
use serde::{Deserialize, Serialize};

use super::flo::FloWrapper;

#[derive(AsU256, Serialize, Deserialize, Clone, PartialEq)]
pub struct AceID(U256);

/// A Verifier is a public key which can verify a signature from a private key.
pub type FloVerifier = FloWrapper<Box<dyn Verifier>>;

/// A badge wraps a Condition with a fixed ID.
pub type FloBadge = FloWrapper<BadgeCond>;

/// An Access Control Entity (ACE) can define fine-grained rules over Flos
/// that define how they can be updated or used.
/// It also gives the possibility to delegate to other ACEs for overwriting
/// or refining rules.
pub type FloACE = FloWrapper<ACE>;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum Rules {
    ACE(Version<AceID>),
    Update(Condition),
    None,
}

/// ACE protect objects with a set of rules useful for the
/// given object.
/// It can:
/// - be updated with new and modified rules
/// - delegate some or all of the rules to another ACE
/// The goal is to have a flexible definition of rules for various
/// sets of objects.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ACE {
    /// How to update a Flo with Rules::ACE(ThisACE)
    update: Option<Condition>,
    /// All rules for this ACE.
    rules: HashMap<String, Condition>,
    /// All delegations are applied in the order as they appear here.
    /// This means that `FullOverwrite` should be at the beginning,
    /// followed by the other delegation types.
    delegation: Vec<ACEDelegation>,
}

/// This is overkill - as a first implementation, only `FillMissing` will be implemented.
#[derive(Clone, Serialize, Deserialize)]
pub enum ACEDelegation {
    /// update and all rules from the given ACE replace the update and rules
    /// with the same name
    FullOverwrite(Version<AceID>),
    /// All rules from the delegated ACE replace the rules with the same name
    RulesOverwrite(Version<AceID>),
    /// Only this rule from the delegated ACE replaces the rule with the same name
    RuleOverwrite(String, Version<AceID>),
    /// Only new rules from the delegated ACE are added
    FillMissing(Version<AceID>),
    /// update and all rules can be used as an alternative Identity
    FullAlternative(Version<AceID>),
    /// All rules from the delegated ACE can be used as alternative Identity
    RulesAlternative(Version<AceID>),
    /// Only this rule from the delegated ACE can be used as an alternative Identity
    /// for the rule with the same name
    RuleAlternative(String, Version<AceID>),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BadgeCond(Condition);

impl ACE {
    pub fn new(
        update: Option<Condition>,
        rules: HashMap<String, Condition>,
        delegation: Vec<ACEDelegation>,
    ) -> Self {
        Self {
            update,
            rules,
            delegation,
        }
    }

    pub fn update(&self) -> Option<Condition> {
        self.update.clone()
    }

    pub fn rules(&self) -> HashMap<String, Condition> {
        self.rules.clone()
    }

    pub fn delegation(&self) -> Vec<ACEDelegation> {
        self.delegation.clone()
    }
}

impl BadgeCond {
    pub fn new(cond: Condition) -> Self {
        Self(cond)
    }

    pub fn cond(&self) -> &Condition{
        &self.0
    }
}

impl Badge for FloBadge {
    fn badge_id(&self) -> BadgeID {
        (*self.flo_id()).into()
    }

    fn badge_cond(&self) -> Condition {
        self.cache().0.clone()
    }

    fn badge_version(&self) -> u32 {
        self.version() as u32
    }

    fn clone_self(&self) -> Box<dyn Badge> {
        Box::new(
            FloBadge::from_type(self.realm_id(), self.rules().clone(), self.cache().clone())
                .unwrap(),
        )
    }
}

impl Rules {
    pub fn update(&self, ace: Option<FloACE>) -> Option<Condition> {
        match self {
            Rules::ACE(version) => ace
                .and_then(|a| version.accepts(a.version()).then(|| a.cache().update()))
                .flatten(),
            Rules::Update(condition) => Some(condition.clone()),
            Rules::None => None,
        }
    }
}
