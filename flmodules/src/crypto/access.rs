use std::collections::HashMap;

use bytes::Bytes;
use flarch::nodeids::U256;
use flarch_macro::AsU256;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::watch;

use crate::{crypto::signer::Signature, flo::flo::ToFromBytes};

use super::signer::{Verifier, VerifierID};

#[derive(AsU256, Serialize, Deserialize, Clone, PartialEq)]
pub struct AceId(U256);

#[derive(AsU256, Serialize, Deserialize, Clone, PartialEq)]
pub struct IdentityID(U256);

#[derive(AsU256, Serialize, Deserialize, Clone, PartialEq)]
pub struct RuleID(U256);

#[derive(Debug)]
pub enum ErrorIdentity {}

/// ACE protect objects with a set of rules useful for the
/// given object.
/// It can:
/// - be updated with new and modified rules
/// - delegate some or all of the rules to another ACE
/// The goal is to have a flexible definition of rules for various
/// sets of objects.
#[derive(Serialize, Deserialize)]
pub struct ACE {
    id: AceId,
    current: ACEData,
    proof: Proof<ACEData>,
    #[serde(skip)]
    ev_cache: Option<watch::Receiver<EVCache>>,
}

/// A Identity can be viewed as an identity or a root of trust.
/// It defines one cryptographic actor which can be composed of
/// many different verifiers and other actors.
/// The main operations of a Identity are:
/// - sign, following the conditions defined in the sign field
/// - update, follosing the conditions defined in the update field
#[derive(Clone, Serialize, Deserialize)]
pub struct Identity {
    id: IdentityID,
    current: IdentityData,
    proof: Proof<IdentityData>,
    #[serde(skip)]
    ev_cache: Option<watch::Receiver<EVCache>>,
}

/// A Condition is a boolean combination of verifiers and identities
/// which can verify a given signature.
/// A Condition can:
/// - sign a given message
/// - verify a given message
#[derive(Clone, Serialize, Deserialize)]
pub enum Condition {
    Verifier(VerifierID),
    Identity(Version<IdentityID>),
    NofT(usize, Vec<Condition>),
    None,
}

/// What versions of the ACE or Identity are accepted.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum Version<T: Serialize + Clone> {
    /// This is the default - everyone should use it and trust future versions.
    Minimal(T, usize),
    /// Paranoid version pinning. Also problematic because old versions might get
    /// forgotten.
    Exact(T, usize),
    /// Just for completeness' sake - this version and all lower version. This doesn't
    /// really make sense.
    Maximal(T, usize),
}

/// A Proof is either a list of all past values, each one followed by a signature
/// to prove the new value is valid.
/// Or it is a signature of a trusted Identity over the ID, the latest values, and the version.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Proof<T: Serialize> {
    Single(Vec<(T, Signature)>),
    Delegated(usize, Vec<(IdentityID, Signature)>),
}

/// ACEData is the variable data of an ACE.
#[derive(Serialize, Deserialize, Clone)]
pub struct ACEData {
    /// Which Identity can be used to verify updates for this ACE.
    pub update: Version<IdentityID>,
    /// All rules for this ACE.
    pub rules: HashMap<String, Version<IdentityID>>,
    /// All delegations are applied in the order as they appear here.
    /// This means that `FullOverwrite` should be at the beginning,
    /// followed by the other delegation types.
    pub delegation: Vec<ACEDelegation>,
}

/// IdentityData is the variable data of an Identity.
#[derive(Clone, Serialize, Deserialize)]
pub struct IdentityData {
    /// The Condition which needs to be met to validate a signature from this Identity.
    sign: Condition,
    /// The Condition which needs to be met to validate an update to this Identity.
    update: Condition,
}

/// This is overkill - as a first implementation, only `FillMissing` will be implemented.
#[derive(Clone, Serialize, Deserialize)]
pub enum ACEDelegation {
    /// update and all rules from the given ACE replace the update and rules
    /// with the same name
    FullOverwrite(Version<AceId>),
    /// All rules from the delegated ACE replace the rules with the same name
    RulesOverwrite(Version<AceId>),
    /// Only this rule from the delegated ACE replaces the rule with the same name
    RuleOverwrite(String, Version<AceId>),
    /// Only new rules from the delegated ACE are added
    FillMissing(Version<AceId>),
    /// update and all rules can be used as an alternative Identity
    FullAlternative(Version<AceId>),
    /// All rules from the delegated ACE can be used as alternative Identity
    RulesAlternative(Version<AceId>),
    /// Only this rule from the delegated ACE can be used as an alternative Identity
    /// for the rule with the same name
    RuleAlternative(String, Version<AceId>),
}

pub struct Identitiesignature {}

#[derive(Default)]
pub struct EVCache {
    identities: HashMap<IdentityID, HashMap<usize, Identity>>,
    verifiers: HashMap<VerifierID, Box<dyn Verifier>>,
}

impl ACE {
    pub fn new(current: ACEData) -> Self {
        Self {
            id: current.calc_id(),
            current,
            proof: Proof::Single(vec![]),
            ev_cache: None,
        }
    }

    pub fn get_id(&self) -> AceId {
        self.id.clone()
    }
}

impl Identity {
    pub fn new(sign: Condition, update: Condition) -> Result<Self, ErrorIdentity> {
        let mut hasher = Sha256::new();
        hasher.update(sign.get_hash());
        hasher.update(update.get_hash());
        Ok(Self {
            id: hasher.finalize().into(),
            current: IdentityData { sign, update },
            proof: Proof::Single(vec![]),
            ev_cache: None,
        })
    }

    pub fn get_id(&self) -> IdentityID {
        self.id.clone()
    }

    pub fn get_version(&self) -> usize {
        self.proof.version()
    }

    pub fn start_signature(&self, msg: Bytes) -> Identitiesignature {
        Identitiesignature {}
    }
}

const RULE_VERIFIER: &str = "verifier";
const RULE_IDENTITY: &str = "identity";
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
            Condition::Identity(e_ver) => {
                hasher.update(RULE_IDENTITY);
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

impl<T: Serialize + Clone> Version<T> {
    pub fn get_id(&self) -> T {
        match self {
            Version::Minimal(id, _) => id.clone(),
            Version::Exact(id, _) => id.clone(),
            Version::Maximal(id, _) => id.clone(),
        }
    }
}

impl<T: Serialize> Proof<T> {
    pub fn version(&self) -> usize {
        match self {
            Proof::Single(vec) => vec.len(),
            Proof::Delegated(len, _) => *len,
        }
    }

    pub fn size(&self) -> usize {
        match self {
            Proof::Single(vec) => vec
                .iter()
                .map(|(t, sig)| rmp_serde::to_vec(t).unwrap().len() + sig.len())
                .sum(),
            Proof::Delegated(ver, vec) => {
                ver.to_be_bytes().len()
                    + vec.iter().map(|(id, b)| id.len() + b.len()).sum::<usize>()
            }
        }
    }
}

impl ACEData {
    pub fn calc_id(&self) -> AceId {
        AceId::hash_into("ican.re.fledg.crypto.version_ace", &[&self.to_bytes()])
    }
}
