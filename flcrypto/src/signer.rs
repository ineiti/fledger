use bytes::Bytes;
use flarch::nodeids::U256;
use flmacro::AsU256;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::signer_ed25519::{SignerEd25519, VerifierEd25519};

pub type Signature = Bytes;

#[derive(Debug, Error, PartialEq)]
pub enum SignerError {
    #[error("Signature doesn't match message")]
    SignatureMessageMismatch,
    #[error("Didn't find Identity")]
    BadgeMissing,
    #[error("Couldn't match message to sign")]
    SigningMessageMismatch,
}

#[derive(AsU256, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct KeyPairID(U256);

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Signer {
    Ed25519(SignerEd25519),
}

impl Signer {
    pub fn sign(&self, msg: &Bytes) -> Result<Signature, SignerError> {
        match self {
            Signer::Ed25519(sig) => sig.sign(msg),
        }
    }

    pub fn verifier(&self) -> Verifier {
        match self {
            Signer::Ed25519(sig) => sig.verifier(),
        }
    }

    pub fn get_id(&self) -> KeyPairID {
        match self {
            Signer::Ed25519(sig) => sig.get_id(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Verifier {
    Ed25519(VerifierEd25519),
}

impl Verifier {
    pub fn verify(&self, msg: &Bytes, sig: &Bytes) -> Result<(), SignerError> {
        match self {
            Verifier::Ed25519(ver) => ver.verify(msg, sig),
        }
    }

    pub fn get_id(&self) -> KeyPairID {
        match self {
            Verifier::Ed25519(ver) => ver.get_id(),
        }
    }
}
