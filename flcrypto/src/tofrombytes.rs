use bytes::Bytes;
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TFBError {
    #[error("Couldn't deserialize {0}: '{1:?}'")]
    Deserialization(String, String),
}

pub trait ToFromBytes: Serialize + DeserializeOwned {
    fn to_rmp_bytes(&self) -> Bytes {
        rmp_serde::to_vec(self).unwrap().into()
    }

    fn from_rmp_bytes(name: &str, bytes: &Bytes) -> anyhow::Result<Self> {
        rmp_serde::from_slice(bytes)
            .map_err(|e| TFBError::Deserialization(name.into(), e.to_string()).into())
    }

    fn size(&self) -> usize {
        self.to_rmp_bytes().len()
    }
}

impl<T: Serialize + DeserializeOwned> ToFromBytes for T {}
