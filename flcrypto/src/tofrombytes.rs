use bytes::Bytes;
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TFBError {
    #[error("Couldn't deserialize {0}: '{1:?}'")]
    Deserialization(String, String),
}

pub trait ToFromBytes: Serialize + DeserializeOwned {
    fn to_bytes(&self) -> Bytes {
        rmp_serde::to_vec(self).unwrap().into()
    }

    fn from_bytes(name: &str, bytes: &Bytes) -> Result<Self, TFBError> {
        rmp_serde::from_slice(bytes)
            .map_err(|e| TFBError::Deserialization(name.into(), e.to_string()))
    }

    fn size(&self) -> usize {
        self.to_bytes().len()
    }
}

impl<T: Serialize + DeserializeOwned> ToFromBytes for T {}
