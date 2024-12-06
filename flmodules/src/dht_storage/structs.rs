use bytes::Bytes;
use num_bigint::BigUint;

use crate::ledger::core::LedgerSetup;
use flarch::nodeids::U256;

pub struct Entry {
    pub id: U256,
    pub valid_until: u64,
    pub data: EntryData,
}

pub enum EntryData {
    Domain(Domain),
    GroupConfig(GroupConfig),
    NodeConfig(NodeConfig),
    LedgerState(LedgerState),
    Blob(Blob),
}

pub struct LedgerState {}

pub struct Domain {
    pub id: U256,
    pub name: String,
    pub ledger: LedgerSetup,
}

pub struct GroupConfig {
    pub name: String,
    pub mana: Mana,
}

pub struct NodeConfig {
    pub name: String,
    pub mana: Mana,
}

pub struct Blob {
    pub name: String,
    pub btype: String,
    pub data: Bytes,
}

pub struct Mana {
    pub epoch: u64,
    pub value: BigUint,
}
