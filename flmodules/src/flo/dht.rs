use serde::{Deserialize, Serialize};

use crate::crypto::access::{AceId, Version};

use super::flo::{Content, Flo, FloError, FloID, ToFromBytes};

pub struct DHTConfig {
    flo: Flo,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DHTStorageConfig {
    // Put the DHTFlos in round(DHTFlo.spread * over_provide) nodes
    pub over_provide: f32,
    // 16 exa bytes should be enough for everybody
    pub max_space: usize,
}

impl DHTConfig {
    pub fn new(ace: Version<AceId>, data: DHTStorageConfig) -> Result<Self, FloError> {
        let flo = Flo::new_now(Content::DHTConfig, data.to_bytes(), ace);
        Ok(Self { flo })
    }

    pub fn to_string(&self) -> Result<String, FloError> {
        match serde_json::to_string(&self.data()?) {
            Ok(str) => Ok(str),
            Err(e) => Err(FloError::Serialization(e.to_string())),
        }
    }

    pub fn data(&self) -> Result<DHTStorageConfig, FloError> {
        DHTStorageConfig::from_bytes("DHTStorageConfig", &self.flo.data)
    }

    pub fn id(&self) -> FloID {
        self.flo.id.clone()
    }

    pub fn ace(&self) -> AceId {
        self.flo.ace.get_id()
    }
}

impl TryFrom<Flo> for DHTConfig {
    type Error = FloError;

    fn try_from(flo: Flo) -> Result<Self, Self::Error> {
        if !matches!(flo.content, Content::DHTConfig) {
            return Err(FloError::WrongContent(Content::DHTConfig, flo.content));
        }

        let d = DHTConfig { flo };
        d.data()?;
        Ok(d)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DHTFlo {
    pub flo: Flo,
    pub spread: u32,
    // TODO: Add the domain of this flo here.
}

impl DHTFlo {
    pub fn new(flo: Flo, spread: u32) -> Self {
        Self { flo, spread }
    }

    pub fn to_string(&self) -> Result<String, FloError> {
        match serde_json::to_string(&self) {
            Ok(str) => Ok(str),
            Err(e) => Err(FloError::Serialization(e.to_string())),
        }
    }

    pub fn id(&self) -> FloID {
        self.flo.id.clone()
    }

    pub fn ace(&self) -> AceId {
        self.flo.ace.get_id()
    }

    pub fn size(&self) -> usize {
        self.flo.size()
    }
}

impl TryFrom<&String> for DHTFlo {
    type Error = FloError;

    fn try_from(str: &String) -> Result<Self, Self::Error> {
        match serde_json::from_str::<DHTFlo>(str) {
            Ok(dht_flo) => {
                dht_flo.flo.is_valid()?;
                Ok(dht_flo)
            }
            Err(e) => Err(FloError::Deserialization("DHTFlo".into(), e.to_string())),
        }
    }
}
