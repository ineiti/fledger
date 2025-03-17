use base64::prelude::*;
use bytes::Bytes;
use enum_dispatch::enum_dispatch;
use flarch::nodeids::U256;
use flmacro::AsU256;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Debug, Display};
use thiserror::Error;

use crate::signer_ed25519::{SignerEd25519, VerifierEd25519};

#[derive(Clone, PartialEq, Debug)]
pub struct Signature(Bytes);

#[derive(Debug, Error, PartialEq)]
pub enum SignerError {
    #[error("No such signer")]
    NoSuchSigner,
    #[error("Not enough signatures")]
    SignaturesMissing,
    #[error("Signature doesn't match message")]
    SignatureMessageMismatch,
    #[error("Didn't find Badge")]
    BadgeMissing,
    #[error("ACE used but not provided")]
    ACEMissing,
    #[error("ACE doesn't have an update rule")]
    ACEMissingUpdate,
    #[error("Cannot update with Rule::Static")]
    UpdateStatic,
    #[error("Couldn't match message to sign")]
    SigningMessageMismatch,
    #[error("PQCrypto error {0}")]
    PQCrypto(String),
}

#[derive(AsU256, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct KeyPairID(U256);

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[enum_dispatch]
pub enum Signer {
    Ed25519(SignerEd25519),
    // MlDSA(SignerMlDSA),
}

impl Debug for Signer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ed25519(arg0) => f.debug_tuple("Ed25519").field(&arg0.get_id()).finish(),
        }
    }
}

impl Display for Signer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{}({})",
            match self {
                Signer::Ed25519(_) => "Ed25519",
            },
            self.get_id()
        ))
    }
}

#[enum_dispatch(Signer)]
pub trait SignerTrait {
    fn sign(&self, msg: &Bytes) -> anyhow::Result<Signature>;

    fn verifier(&self) -> Verifier;

    fn get_id(&self) -> KeyPairID;
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[enum_dispatch]
pub enum Verifier {
    Ed25519(VerifierEd25519),
    // MlDSA(VerifierMlDSA),
}

impl Debug for Verifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ed25519(arg0) => f.debug_tuple("Ed25519").field(&arg0.get_id()).finish(),
        }
    }
}

impl Display for Verifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{}({})",
            match self {
                Verifier::Ed25519(_) => "Ed25519",
            },
            self.get_id()
        ))
    }
}

#[enum_dispatch(Verifier)]
pub trait VerifierTrait {
    fn verify(&self, msg: &Bytes, sig: &Signature) -> anyhow::Result<()>;

    fn get_id(&self) -> KeyPairID;
}

impl Signature {
    pub fn bytes(&self) -> &Bytes {
        &self.0
    }
}

// Implement Serialize trait for base64 encoding
impl Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let base64_str = BASE64_STANDARD.encode(&self.0);
        serializer.serialize_str(&base64_str)
    }
}

// Implement Deserialize trait for base64 decoding
impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        let s = String::deserialize(deserializer)?;
        let bytes = BASE64_STANDARD.decode(s).map_err(Error::custom)?;
        Ok(Signature(Bytes::from(bytes)))
    }
}

impl AsRef<Bytes> for Signature {
    fn as_ref(&self) -> &Bytes {
        &self.0
    }
}

impl From<Vec<u8>> for Signature {
    fn from(value: Vec<u8>) -> Self {
        Signature(value.into())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn serialize_signature() -> anyhow::Result<()> {
        let sig = Signature(Bytes::from_static(b"1234"));
        let sig_str = serde_yaml::to_string(&sig)?;
        let sig2: Signature = serde_yaml::from_str(&sig_str)?;
        assert_eq!(sig, sig2);
        Ok(())
    }
}
