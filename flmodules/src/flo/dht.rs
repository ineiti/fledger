use flarch::nodeids::U256;
use serde::{Deserialize, Serialize};

use super::flo::{Content, Flo, FloError, ACE};

pub struct DHTConfig {
    flo: Flo,
    pub data: DHTStorageConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DHTStorageConfig {
    // Put the DHTFlos in round(DHTFlo.spread * over_provide) nodes
    pub over_provide: f32,
    // 16 exa bytes should be enough for everybody
    pub max_space: u64,
}

impl DHTConfig {
    pub fn new(ace: ACE, data: DHTStorageConfig) -> Result<Self, FloError> {
        let flo = Flo::new_now(
            Content::DHTConfig,
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

impl TryFrom<Flo> for DHTConfig {
    type Error = FloError;

    fn try_from(flo: Flo) -> Result<Self, Self::Error> {
        if !matches!(flo.content, Content::DHTConfig) {
            return Err(FloError::WrongContent(Content::DHTConfig, flo.content));
        }

        match serde_json::from_str::<DHTStorageConfig>(&flo.data()) {
            Ok(data) => Ok(DHTConfig { flo, data }),
            Err(e) => Err(FloError::Deserialization("DHTConfig".into(), e.to_string())),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DHTFlo {
    pub flo: Flo,
    pub spread: u32,
}

impl DHTFlo {
    pub fn new(flo: Flo, spread: u32) -> Result<Self, FloError> {
        Ok(Self { flo, spread })
    }

    pub fn to_string(&self) -> Result<String, FloError> {
        match serde_json::to_string(&self) {
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

impl TryFrom<&String> for DHTFlo {
    type Error = FloError;

    fn try_from(str: &String) -> Result<Self, Self::Error> {
        match serde_json::from_str::<DHTFlo>(str) {
            Ok(dht_flo) => Ok(dht_flo),
            Err(e) => Err(FloError::Deserialization("DHTFlo".into(), e.to_string())),
        }
    }
}
