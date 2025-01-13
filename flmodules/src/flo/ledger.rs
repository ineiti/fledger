use flarch::nodeids::NodeID;
use serde::{Deserialize, Serialize};

use crate::crypto::signer::Verifier;

use super::flo::FloWrapper;

pub type LedgerConfig = FloWrapper<LedgerConfigData>;

#[derive(Debug, Serialize, Deserialize)]
pub struct LedgerConfigData {
    pub nodes: Vec<LedgerNode>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LedgerNode {
    pub id: NodeID,
    pub verifier: Box<dyn Verifier>,
}

impl LedgerConfig {}
