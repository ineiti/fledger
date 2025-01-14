use std::any::type_name;

use bytes::Bytes;
use flarch::nodeids::U256;
use flarch_macro::AsU256;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::access::{AceId, History, Version};

#[derive(Debug, Error)]
pub enum FloError {
    #[error("Couldn't deserialize {0}: '{1:?}'")]
    Deserialization(String, String),
    #[error("Couldn't serialize: {0}")]
    Serialization(String),
    #[error("Expected content to be '{0:?}', but was '{1:?}")]
    WrongContent(String, String),
    #[error("While converting flo to {0:?}: {1}")]
    Conversion(String, String),
    #[error("This update has no Data")]
    UpdateNoData,
    #[error("This update has no ACE")]
    UpdateNoACE,
    #[error("No Updates")]
    UpdatesMissing,
}

#[derive(AsU256, Deserialize, Serialize, Clone, PartialEq, Eq, Hash)]
pub struct FloID(U256);

/// Flo defines the following actions:
/// - ican.re.fledg.flmodules.flo.flo
///   - .update_data - update the data
///   - .update_ace - update the Version<AceId>
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Flo {
    pub id: FloID,
    pub content: String,
    pub current: FloData,
    pub history: History<FloData>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct FloWrapper<T> {
    flo: Flo,
    cache: T,
}

/// Modifications applied to a Flo.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct FloData {
    pub data: Bytes,
    pub ace: Version<AceId>,
}

impl Flo {
    pub fn new(content: String, data: Bytes, ace: Version<AceId>) -> Self {
        Self {
            id: FloID::hash_into(
                "ican.re.fledg.flmodules.flo.Flo",
                &[&content.to_bytes(), &data, &ace.to_bytes()],
            ),
            content,
            current: FloData { data, ace },
            history: History::Single(vec![]),
        }
    }

    pub fn data(&self) -> Bytes {
        self.current.data.clone()
    }

    pub fn ace_id(&self) -> AceId {
        self.current.ace.get_id()
    }

    pub fn is_valid(&self) -> Result<(), FloError> {
        todo!()
    }

    pub fn version(&self) -> usize {
        self.history.version()
    }
}

impl TryFrom<String> for Flo {
    type Error = FloError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let flo = match serde_yaml::from_str::<Self>(&value) {
            Ok(flo) => flo,
            Err(e) => return Err(FloError::Deserialization("Flo".into(), e.to_string())),
        };
        flo.is_valid()?;
        Ok(flo)
    }
}

pub trait ToFromBytes: Serialize + DeserializeOwned {
    fn to_bytes(&self) -> Bytes {
        rmp_serde::to_vec(self).unwrap().into()
    }

    fn from_bytes(name: &str, bytes: &Bytes) -> Result<Self, FloError> {
        rmp_serde::from_slice(bytes)
            .map_err(|e| FloError::Deserialization(name.into(), e.to_string()))
    }

    fn size(&self) -> usize {
        self.to_bytes().len()
    }
}

impl<T: Serialize + DeserializeOwned> ToFromBytes for T {}

impl<T: Serialize + DeserializeOwned + Clone> FloWrapper<T> {
    pub fn new(ace: Version<AceId>, cache: T) -> Result<Self, FloError> {
        let flo = Flo::new(type_name::<T>().into(), cache.to_bytes(), ace);
        Ok(Self { flo, cache })
    }

    pub fn to_string(&self) -> Result<String, FloError> {
        match serde_json::to_string(&self.cache) {
            Ok(str) => Ok(str),
            Err(e) => Err(FloError::Serialization(e.to_string())),
        }
    }

    pub fn cache(&self) -> T {
        self.cache.clone()
    }

    pub fn flo(&self) -> Flo {
        self.flo.clone()
    }

    pub fn id(&self) -> FloID {
        self.flo.id.clone()
    }

    pub fn ace(&self) -> AceId {
        self.flo.current.ace.get_id()
    }
}

impl<T: Serialize + DeserializeOwned> TryFrom<Flo> for FloWrapper<T> {
    type Error = FloError;

    fn try_from(flo: Flo) -> Result<Self, Self::Error> {
        let content = type_name::<T>().to_string();
        if flo.content != content {
            return Err(FloError::WrongContent(content, flo.content));
        }

        let cache = T::from_bytes("name", &flo.current.data)?;

        Ok(Self { flo, cache })
    }
}
