use serde::{Deserialize, Serialize};

use super::flo::FloWrapper;

pub type FloEntity = FloWrapper<FloEntityData>;

#[derive(Debug, Serialize, Deserialize)]
pub struct FloEntityData {}

impl FloEntity {}
