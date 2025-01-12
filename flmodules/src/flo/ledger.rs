use flarch::nodeids::{NodeID, U256};
use serde::{Deserialize, Serialize};

use crate::crypto::signer::Verifier;

use super::flo::{Content, Flo, FloError, ACE};

pub struct LedgerConfig {
    flo: Flo,
    pub data: LedgerConfigData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LedgerConfigData {
    pub nodes: Vec<LedgerNode>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LedgerNode {
    pub id: NodeID,
    pub verifier: Box<dyn Verifier>,
}

impl LedgerConfig {
    pub fn new(ace: ACE, data: LedgerConfigData) -> Result<Self, FloError> {
        let flo = Flo::new_now(
            Content::LedgerConfig,
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

impl TryFrom<Flo> for LedgerConfig {
    type Error = FloError;

    fn try_from(flo: Flo) -> Result<Self, Self::Error> {
        if !matches!(flo.content, Content::LedgerConfig) {
            return Err(FloError::WrongContent(Content::LedgerConfig, flo.content));
        }

        match serde_json::from_str::<LedgerConfigData>(&flo.data()) {
            Ok(data) => Ok(LedgerConfig { flo, data }),
            Err(e) => Err(FloError::Deserialization(
                "LedgerConfig".into(),
                e.to_string(),
            )),
        }
    }
}
