use num_bigint::BigUint;
use serde::{Deserialize, Serialize};

use crate::crypto::access::{AceId, Version};

use super::flo::{Content, Flo, FloError, FloID, ToFromBytes};

pub struct Mana {
    flo: Flo,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ManaData {
    pub amount: BigUint,
}

impl Mana {
    pub fn new(ace: Version<AceId>, data: ManaData) -> Result<Self, FloError> {
        let flo = Flo::new_now(Content::Mana, data.to_bytes(), ace);
        Ok(Self { flo })
    }

    pub fn to_string(&self) -> Result<String, FloError> {
        match serde_json::to_string(&self.data()?) {
            Ok(str) => Ok(str),
            Err(e) => Err(FloError::Serialization(e.to_string())),
        }
    }

    pub fn data(&self) -> Result<ManaData, FloError> {
        ManaData::from_bytes("ManaData", &self.flo.data)
    }

    pub fn id(&self) -> FloID {
        self.flo.id.clone()
    }

    pub fn ace(&self) -> AceId {
        self.flo.ace.get_id()
    }
}

impl TryFrom<Flo> for Mana {
    type Error = FloError;

    fn try_from(flo: Flo) -> Result<Self, Self::Error> {
        if !matches!(flo.content, Content::Mana) {
            return Err(FloError::WrongContent(Content::Mana, flo.content));
        }

        let m = Mana { flo };
        m.data()?;
        Ok(m)
    }
}
