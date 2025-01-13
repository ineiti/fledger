use flarch::nodeids::NodeID;
use serde::{Deserialize, Serialize};

use crate::crypto::{
    access::{AceId, Version},
    signer::Verifier,
};

use super::flo::{Content, Flo, FloError, ToFromBytes};

pub struct LedgerConfig {
    flo: Flo,
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
    pub fn new(ace: Version<AceId>, data: LedgerConfigData) -> Result<Self, FloError> {
        let flo = Flo::new_now(Content::LedgerConfig, data.to_bytes(), ace);
        Ok(Self { flo })
    }

    pub fn to_string(&self) -> Result<String, FloError> {
        match serde_json::to_string(&self.data()?) {
            Ok(str) => Ok(str),
            Err(e) => Err(FloError::Serialization(e.to_string())),
        }
    }

    pub fn data(&self) -> Result<LedgerConfigData, FloError> {
        LedgerConfigData::from_bytes("LedgerConfigData", &self.flo.data)
    }

    pub fn id(&self) -> super::flo::FloID {
        self.flo.id.clone()
    }

    pub fn ace(&self) -> AceId {
        self.flo.ace.get_id()
    }
}

impl TryFrom<Flo> for LedgerConfig {
    type Error = FloError;

    fn try_from(flo: Flo) -> Result<Self, Self::Error> {
        if !matches!(flo.content, Content::LedgerConfig) {
            return Err(FloError::WrongContent(Content::LedgerConfig, flo.content));
        }

        let l = LedgerConfig { flo };
        l.data()?;
        Ok(l)
    }
}
