use bytes::Bytes;
use flarch::nodeids::U256;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

use super::flo::{Content, Flo, FloError, ACE};

pub struct Blob {
    flo: Flo,
    pub data: BlobData,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
pub struct BlobData {
    #[serde_as(as = "Base64")]
    pub bytes: Bytes,
    pub content: String,
}

impl Blob {
    pub fn new(ace: ACE, data: BlobData) -> Result<Self, FloError> {
        let flo = Flo::new_now(
            Content::Blob,
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

impl TryFrom<Flo> for Blob {
    type Error = FloError;

    fn try_from(flo: Flo) -> Result<Self, Self::Error> {
        if !matches!(flo.content, Content::Blob) {
            return Err(FloError::WrongContent(Content::Blob, flo.content));
        }

        match serde_json::from_str::<BlobData>(&flo.data()) {
            Ok(data) => Ok(Blob { flo, data }),
            Err(e) => Err(FloError::Deserialization("Blob".into(), e.to_string())),
        }
    }
}
