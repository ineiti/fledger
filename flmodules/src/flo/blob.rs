use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

use crate::crypto::access::{AceId, Version};

use super::flo::{Content, Flo, FloError, FloID};

pub struct Blob {
    flo: Flo,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
pub struct BlobData {
    #[serde_as(as = "Base64")]
    pub bytes: Bytes,
    pub content: String,
}

impl Blob {
    pub fn new(ace: Version<AceId>, data: BlobData) -> Result<Self, FloError> {
        let flo = Flo::new_now(Content::Blob, data.to_bytes(), ace);
        Ok(Self { flo })
    }

    pub fn to_string(&self) -> Result<String, FloError> {
        match serde_json::to_string(&self.data()) {
            Ok(str) => Ok(str),
            Err(e) => Err(FloError::Serialization(e.to_string())),
        }
    }

    pub fn data(&self) -> BlobData {
        (&self.flo.data).try_into().unwrap()
    }

    pub fn id(&self) -> FloID {
        self.flo.id.clone()
    }

    pub fn ace(&self) -> AceId {
        self.flo.ace.get_id()
    }
}

impl TryFrom<Flo> for Blob {
    type Error = FloError;

    fn try_from(flo: Flo) -> Result<Self, Self::Error> {
        if !matches!(flo.content, Content::Blob) {
            return Err(FloError::WrongContent(Content::Blob, flo.content));
        }

        Ok(Blob { flo })
    }
}

impl BlobData {
    pub fn to_bytes(&self) -> Bytes {
        rmp_serde::to_vec(self).unwrap().into()
    }
}

impl TryFrom<&Bytes> for BlobData {
    type Error = FloError;

    fn try_from(value: &Bytes) -> Result<Self, Self::Error> {
        rmp_serde::from_slice(&value)
            .map_err(|e| FloError::Deserialization("BlobData".into(), e.to_string()))
    }
}
