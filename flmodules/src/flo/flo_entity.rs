use serde::{Deserialize, Serialize};

use crate::crypto::access::{AceId, Version};

use super::flo::{Content, Flo, FloError, FloID, ToFromBytes};

pub struct FloEntity {
    flo: Flo,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FloEntityData {}

impl FloEntity {
    pub fn new(ace: Version<AceId>, data: FloEntityData) -> Result<Self, FloError> {
        let flo = Flo::new_now(Content::FloEntity, data.to_bytes(), ace);
        Ok(Self { flo })
    }

    pub fn to_string(&self) -> Result<String, FloError> {
        match serde_json::to_string(&self.data()?) {
            Ok(str) => Ok(str),
            Err(e) => Err(FloError::Serialization(e.to_string())),
        }
    }

    pub fn data(&self) -> Result<FloEntityData, FloError>{
        FloEntityData::from_bytes("FloEntityData", &self.flo.data)
    }

    pub fn id(&self) -> FloID {
        self.flo.id.clone()
    }

    pub fn ace(&self) -> AceId {
        self.flo.ace.get_id()
    }
}

impl TryFrom<Flo> for FloEntity {
    type Error = FloError;

    fn try_from(flo: Flo) -> Result<Self, Self::Error> {
        if !matches!(flo.content, Content::FloEntity) {
            return Err(FloError::WrongContent(Content::FloEntity, flo.content));
        }

        let f = FloEntity{flo};
        f.data()?;
        Ok(f)
    }
}
