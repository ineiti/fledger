use flarch::nodeids::U256;
use serde::{Deserialize, Serialize};

use super::flo::{Content, Flo, FloError, ACE};

pub struct FloEntity {
    flo: Flo,
    pub data: FloEntityData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FloEntityData {
}

impl FloEntity {
    pub fn new(ace: ACE, data: FloEntityData) -> Result<Self, FloError> {
        let flo = Flo::new_now(
            Content::FloEntity,
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

impl TryFrom<Flo> for FloEntity {
    type Error = FloError;

    fn try_from(flo: Flo) -> Result<Self, Self::Error> {
        if !matches!(flo.content, Content::FloEntity) {
            return Err(FloError::WrongContent(Content::FloEntity, flo.content));
        }

        match serde_json::from_str::<FloEntityData>(&flo.data()) {
            Ok(data) => Ok(FloEntity { flo, data }),
            Err(e) => Err(FloError::Deserialization("FloEntity".into(), e.to_string())),
        }
    }
}
