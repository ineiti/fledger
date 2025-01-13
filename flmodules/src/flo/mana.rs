use num_bigint::BigUint;
use serde::{Deserialize, Serialize};

use super::flo::FloWrapper;

pub type Mana = FloWrapper<ManaData>;

#[derive(Debug, Serialize, Deserialize)]
pub struct ManaData {
    pub amount: BigUint,
}

impl Mana {}
