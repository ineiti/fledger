use flarch::nodeids::U256;
use serde::{Deserialize, Serialize};

use crate::crypto::access::{AceId, Version};

use super::{
    dht::DHTStorageConfig,
    flo::{Content, Flo, FloError, FloID, ToFromBytes},
    ledger::LedgerConfigData,
};

pub struct Domain {
    flo: Flo,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DomainData {
    pub name: String,
    pub dht: Option<DHTStorageConfig>,
    pub ledger: Option<LedgerConfigData>,
    pub sub_domains: Vec<U256>,
}

impl Domain {
    pub fn new(ace: Version<AceId>, data: DomainData) -> Result<Self, FloError> {
        let flo = Flo::new_now(Content::Domain, data.to_bytes(), ace);
        Ok(Self { flo })
    }

    pub fn to_string(&self) -> Result<String, FloError> {
        match serde_json::to_string(&self.data()?) {
            Ok(str) => Ok(str),
            Err(e) => Err(FloError::Serialization(e.to_string())),
        }
    }

    pub fn data(&self) -> Result<DomainData, FloError> {
        DomainData::from_bytes("DomainData", &self.flo.data)
    }

    pub fn id(&self) -> FloID {
        self.flo.id.clone()
    }

    pub fn ace(&self) -> AceId {
        self.flo.ace.get_id()
    }
}

impl TryFrom<Flo> for Domain {
    type Error = FloError;

    fn try_from(flo: Flo) -> Result<Self, Self::Error> {
        if !matches!(flo.content, Content::Domain) {
            return Err(FloError::WrongContent(Content::Domain, flo.content));
        }

        let d = Domain { flo };
        d.data()?;
        Ok(d)
    }
}
