use flarch::nodeids::U256;
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};

use super::flo::{Content, Flo, FloError, ACE};

pub struct Mana {
    flo: Flo,
    pub data: ManaData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ManaData {
    pub amount: BigUint,
}

impl Mana {
    pub fn new(ace: ACE, data: ManaData) -> Result<Self, FloError> {
        let flo = Flo::new_now(
            Content::Mana,
            serde_json::to_string(&data).map_err(|e| FloError::Serialization(e.to_string()))?,
            ace,
        );
        Ok(Self { flo, data })
    }

    pub fn to_string(&self) -> Result<String, FloError> {
        match serde_json::to_string(&self.data) {
            Ok(str) => Ok(str),
            Err(e) => Err(FloError::Serialization(e.to_string())),
        }
    }

    pub fn id(&self) -> U256 {
        self.flo.id
    }

    pub fn ace(&self) -> ACE {
        self.flo.ace()
    }
}

impl TryFrom<Flo> for Mana {
    type Error = FloError;

    fn try_from(flo: Flo) -> Result<Self, Self::Error> {
        if !matches!(flo.content, Content::Mana) {
            return Err(FloError::WrongContent(Content::Mana, flo.content));
        }

        match serde_json::from_str::<ManaData>(&flo.data()) {
            Ok(data) => Ok(Mana { flo, data }),
            Err(e) => Err(FloError::Deserialization("Mana".into(), e.to_string())),
        }
    }
}
