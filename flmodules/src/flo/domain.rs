use flarch::nodeids::U256;
use serde::{Deserialize, Serialize};

use super::{dht::DHTConfigData, flo::FloWrapper, ledger::LedgerConfigData};

pub type Domain = FloWrapper<DomainData>;

#[derive(Debug, Serialize, Deserialize)]
pub struct DomainData {
    pub name: String,
    pub dht: Option<DHTConfigData>,
    pub ledger: Option<LedgerConfigData>,
    pub sub_domains: Vec<U256>,
}

impl Domain {}
