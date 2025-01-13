use bytes::Bytes;
use flarch::{nodeids::U256, tasks::now};
use flarch_macro::AsU256;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::access::{AceId, Proof, Version};

#[derive(Debug, Error)]
pub enum FloError {
    #[error("Couldn't deserialize {0}: '{1:?}'")]
    Deserialization(String, String),
    #[error("Couldn't serialize: {0}")]
    Serialization(String),
    #[error("Expected content to be '{0:?}', but was '{1:?}")]
    WrongContent(Content, Content),
    #[error("While converting flo to {0:?}: {1}")]
    Conversion(Content, String),
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
/// - ican.re.fledg.flo.flo
///   - .update_data - change the data of the Flo
///   - .update_ace -
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Flo {
    pub id: FloID,
    pub content: Content,
    pub data: Bytes,
    pub ace: Version<AceId>,
    pub proof: Proof<Change>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Default, Clone)]
pub enum Content {
    #[default]
    Domain,
    DHTConfig,
    FloEntity,
    LedgerConfig,
    Mana,
    Blob,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum Change {
    Data(u64, Bytes),
    ACE(u64, AceId),
}

impl Flo {
    pub fn new_now(content: Content, data: Bytes, ace: Version<AceId>) -> Self {
        Self::new(now() as u64, content, data, ace)
    }

    pub fn new(now: u64, content: Content, data: Bytes, ace: Version<AceId>) -> Self {
        Self {
            id: FloID::hash_into(
                "ican.re.fledg.flo.Flo",
                &[
                    &now.to_be_bytes(),
                    &content.to_bytes(),
                    &data,
                    &ace.to_bytes(),
                ],
            ),
            content,
            data,
            ace,
            proof: Proof::Single(vec![]),
        }
    }

    pub fn data(&self) -> Bytes {
        self.data.clone()
    }

    pub fn ace_id(&self) -> AceId {
        self.ace.get_id()
    }

    pub fn is_valid(&self) -> Result<(), FloError> {
        todo!()
    }

    pub fn version(&self) -> usize {
        self.proof.version()
    }

    pub fn size(&self) -> usize {
        self.id.0.to_bytes().len() +
        self.content.to_bytes().len() +
        self.data.len() +
        self.ace.to_bytes().len() +
        self.proof.size()
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

impl Content {
    pub fn to_bytes(&self) -> Bytes {
        format!("{:?}", self).into()
    }
}

impl Change {
    pub fn data(&self) -> Result<Bytes, FloError> {
        match self {
            Change::Data(_, b) => Ok(b.clone()),
            Change::ACE(_, _) => Err(FloError::UpdateNoData),
        }
    }

    pub fn ace(&self) -> Result<AceId, FloError> {
        match self {
            Change::ACE(_, ace) => Ok(ace.clone()),
            Change::Data(_, _) => Err(FloError::UpdateNoACE),
        }
    }

    pub fn size(&self) -> usize {
        match self {
            Change::Data(_, b) => b.len(),
            Change::ACE(_, ace) => ace.to_bytes().len(),
        }
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
}

impl<T: Serialize + DeserializeOwned> ToFromBytes for T {}
