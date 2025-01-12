use std::collections::HashMap;

use bytes::Bytes;
use flarch::nodeids::U256;
use flarch_macro::AsU256;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::watch;

use crate::crypto::signer::Signature;

use super::signer::{Verifier, VerifierID};

#[derive(AsU256, Serialize, Deserialize, Clone, PartialEq)]
pub struct AccessRulesID(U256);

#[derive(AsU256, Serialize, Deserialize, Clone, PartialEq)]
pub struct TesseraID(U256);

#[derive(AsU256, Serialize, Deserialize, Clone, PartialEq)]
pub struct RuleID(U256);

#[derive(Debug)]
pub enum ErrorTessera {}

// AccessRules protect objects with a set of rules useful for the
// given object.
// It can:
// - be updated with new and modified rules
// - delegate some or all of the rules to another AccessRules
// The goal is to have a flexible definition of rules for various
// sets of objects.
#[derive(Serialize, Deserialize)]
pub struct AccessRules {
    id: AccessRulesID,
    current: VersionAccessRule,
    proof: Proof<VersionAccessRule>,
    #[serde(skip)]
    ev_cache: Option<watch::Receiver<EVCache>>,
}

// A Tessera can be viewed as an identity or a root of trust.
// It defines one cryptographic actor which can be composed of
// many different verifiers and other actors.
// The main operations of a Tessera are:
// - sign, following the conditions defined in the sign field
// - update, follosing the conditions defined in the update field
#[derive(Clone, Serialize, Deserialize)]
pub struct Tessera {
    id: TesseraID,
    current: VersionTessera,
    proof: Proof<VersionTessera>,
    #[serde(skip)]
    ev_cache: Option<watch::Receiver<EVCache>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum TesseraVersion {
    Exact(TesseraID, usize),
    Minimal(TesseraID, usize),
    Maximal(TesseraID, usize),
}

// A Condition is a boolean combination of verifiers and tesseras
// which can verify a given signature.
// A Condition can:
// - sign a given message
// - verify a given message
#[derive(Clone, Serialize, Deserialize)]
pub enum Condition {
    Verifier(VerifierID),
    Tessera(TesseraVersion),
    NofT(usize, Vec<Condition>),
    None,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum Proof<T> {
    Single(Vec<(T, Signature)>),
    Delegated(usize, Vec<(TesseraID, Signature)>),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct VersionAccessRule {
    update: TesseraVersion,
    rules: HashMap<String, TesseraVersion>,
    // All delegations are applied in the order as they appear here.
    // This means that `FullOverwrite` should be at the beginning,
    // followed by the other delegation types.
    delegation: Vec<AccessRulesDelegation>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct VersionTessera {
    sign: Condition,
    update: Condition,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum AccessRulesDelegation {
    // update and all rules from the given AccessRules replace the update and rules
    // with the same name
    FullOverwrite(AccessRulesID),
    // All rules from the delegated AccessRules replace the rules with the same name
    RulesOverwrite(AccessRulesID),
    // Only this rule from the delegated AccessRules replaces the rule with the same name
    RuleOverwrite(String, AccessRulesID),
    // Only new rules from the delegated AccessRules are added
    FillMissing(AccessRulesID),
    // update and all rules can be used as an alternative Tessera
    FullAlternative(AccessRulesID),
    // All rules from the delegated AccessRules can be used as alternative Tessera
    RulesAlternative(AccessRulesID),
    // Only this rule from the delegated AccessRules can be used as an alternative Tessera
    // for the rule with the same name
    RuleAlternative(String, AccessRulesID),
}

pub struct TesseraSignature {}

#[derive(Default)]
pub struct EVCache {
    tesseras: HashMap<TesseraID, HashMap<usize, Tessera>>,
    verifiers: HashMap<VerifierID, Box<dyn Verifier>>,
}

impl Tessera {
    pub fn new(sign: Condition, update: Condition) -> Result<Self, ErrorTessera> {
        let mut hasher = Sha256::new();
        hasher.update(sign.get_hash());
        hasher.update(update.get_hash());
        Ok(Self {
            id: hasher.finalize().into(),
            current: VersionTessera{ sign, update },
            proof: Proof::Single(vec![]),
            ev_cache: None,
        })
    }

    pub fn get_id(&self) -> TesseraID {
        self.id.clone()
    }

    pub fn get_version(&self) -> usize {
        self.proof.version()
    }

    pub fn start_signature(&self, msg: Bytes) -> TesseraSignature {
        TesseraSignature {}
    }
}

const RULE_VERIFIER: &str = "verifier";
const RULE_TESSERA: &str = "tessera";
const RULE_NOFT: &str = "noft";
const RULE_NONE: &str = "none";

impl Condition {
    fn get_hash(&self) -> RuleID {
        let mut hasher = Sha256::new();
        match self {
            Condition::Verifier(v_id) => {
                hasher.update(RULE_VERIFIER);
                hasher.update(v_id.to_bytes());
            }
            Condition::Tessera(e_ver) => {
                hasher.update(RULE_TESSERA);
                hasher.update(e_ver.to_bytes());
            }
            Condition::NofT(n, rules) => {
                hasher.update(RULE_NOFT);
                hasher.update(n.to_le_bytes());
                for rule in rules {
                    hasher.update(rule.get_hash());
                }
            }
            Condition::None => hasher.update(RULE_NONE),
        }
        hasher.finalize().into()
    }
}

impl TesseraVersion {
    pub fn to_bytes(&self) -> Bytes {
        todo!()
    }
}

impl<T> Proof<T> {
    pub fn version(&self) -> usize {
        match self{
            Proof::Single(vec) => vec.len(),
            Proof::Delegated(len, _) => *len,
        }
    }
}