use flarch::nodeids::U256;
use serde::{Deserialize, Serialize};

use super::{
    dht::DHTStorageConfig,
    flo::{ACE, Content, Flo, FloError},
    ledger::LedgerConfigData,
};

pub struct Domain {
    flo: Flo,
    pub data: DomainData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DomainData {
    pub name: String,
    pub dht: Option<DHTStorageConfig>,
    pub ledger: Option<LedgerConfigData>,
    pub sub_domains: Vec<U256>,
}

impl Domain {
    pub fn new(ace: ACE, data: DomainData) -> Result<Self, FloError> {
        let flo = Flo::new_now(
            Content::Domain,
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

impl TryFrom<Flo> for Domain {
    type Error = FloError;

    fn try_from(flo: Flo) -> Result<Self, Self::Error> {
        if !matches!(flo.content, Content::Domain) {
            return Err(FloError::WrongContent(Content::Domain, flo.content));
        }

        match serde_json::from_str::<DomainData>(&flo.data()) {
            Ok(data) => Ok(Domain { flo, data }),
            Err(e) => Err(FloError::Deserialization("Domain".into(), e.to_string())),
        }
    }
}
