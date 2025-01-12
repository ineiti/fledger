use std::collections::HashMap;

use bytes::Bytes;
use flarch::nodeids::U256;
use sha2::{Digest, Sha256};

use super::signer::Verifier;

pub type ID = U256;
pub type ExpressionID = U256;

#[derive(Debug)]
pub enum ErrorExpression {
    WrongExpressions,
}

pub struct AccessRules {
    id: ID,
    update: ExpressionID,
    rules: HashMap<String, ExpressionID>,
    updates: Vec<AccessRuleUpdate>,
}

pub struct AccessRuleUpdate {}

#[derive(Clone)]
pub struct Expression {
    id: ID,
    rr: RequiredRules,
    rules: HashMap<String, Rule>,
    updates: Vec<ExpressionUpdate>,
}

#[derive(Clone)]
pub enum Rule {
    Verifier(ID),
    Expression(ID),
    NofT(usize, Vec<Expression>),
    None,
}

impl Expression {
    pub fn new(rules: HashMap<String, Rule>) -> Result<Self, ErrorExpression> {
        Ok(Self {
            id: Self::calc_id(&rules),
            rr: RequiredRules::from_keys(rules.keys().collect()),
            rules,
            updates: vec![],
        })
    }

    pub fn new_signer_updater(sign: Rule, update: Rule) -> Self {
        let rules = [
            (RULE_SIGN.to_string(), sign),
            (RULE_UPDATE.to_string(), update),
        ]
        .iter()
        .cloned()
        .collect();
        Self::new(rules).unwrap()
    }

    pub fn has_sign(&self) -> bool {
        self.rr == RequiredRules::Signer || self.rr == RequiredRules::SignerUpdater
    }

    pub fn has_update(&self) -> bool {
        self.rr == RequiredRules::Updater || self.rr == RequiredRules::SignerUpdater
    }

    pub fn get_id(&self) -> ID {
        self.id
    }

    pub fn get_version(&self) -> usize {
        self.updates.len()
    }

    pub fn start_signature(&self, msg: Bytes) -> ExpressionSignature {
        ExpressionSignature {}
    }

    fn calc_id(rules: &HashMap<String, Rule>) -> ID {
        let mut hasher = Sha256::new();
        for (k, v) in rules {
            hasher.update(k.len().to_le_bytes());
            hasher.update(v.get_id())
        }
        hasher.finalize().into()
    }
}

const RULE_SIGN: &str = "sign";
const RULE_UPDATE: &str = "update";
const RULE_VERIFIER: &str = "verifier";
const RULE_EXPRESSION: &str = "expression";
const RULE_NOFT: &str = "noft";
const RULE_NONE: &str = "none";

impl Rule {
    fn get_id(&self) -> ID {
        let mut hasher = Sha256::new();
        match self {
            Rule::Verifier(u256) => {
                hasher.update(RULE_VERIFIER);
                hasher.update(u256.to_bytes());
            }
            Rule::Expression(u256) => {
                hasher.update(RULE_EXPRESSION);
                hasher.update(u256.to_bytes());
            }
            Rule::NofT(n, vec) => {
                hasher.update(RULE_NOFT);
                hasher.update(n.to_le_bytes());
                for exp in vec {
                    hasher.update(exp.get_id());
                }
            }
            Rule::None => hasher.update(RULE_NONE),
        }
        hasher.finalize().into()
    }
}

#[derive(Clone, PartialEq)]
pub enum RequiredRules {
    Signer,
    SignerUpdater,
    Updater,
    None,
}

impl RequiredRules {
    pub fn from_sig_up(signer: bool, update: bool) -> Self {
        if signer {
            if update {
                Self::SignerUpdater
            } else {
                Self::Signer
            }
        } else {
            if update {
                Self::Updater
            } else {
                Self::None
            }
        }
    }

    pub fn from_keys(keys: Vec<&String>) -> Self {
        Self::from_sig_up(
            keys.contains(&&"sign".to_string()),
            keys.contains(&&"update".to_string()),
        )
    }
}

#[derive(Clone)]
pub struct ExpressionUpdate {
    rules: HashMap<String, Rule>,
    signature: Bytes,
}

pub struct ExpressionSignature {}

pub struct CryptoBox {
    expressions: Vec<Expression>,
    verifiers: Vec<Box<dyn Verifier>>,
}
