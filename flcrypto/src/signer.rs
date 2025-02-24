use bytes::Bytes;
use flarch::{broker::asy::Async, nodeids::U256};
use flmacro::AsU256;
use serde::{Deserialize, Serialize};
use thiserror::Error;

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

#[typetag::serde(tag = "type")]
pub trait Signer: std::fmt::Debug {
    fn sign(&self, msg: &Bytes) -> Result<Signature, SignerError>;

    fn verifier(&self) -> Box<dyn Verifier>;

    fn get_id(&self) -> KeyPairID;

    fn clone(&self) -> Box<dyn Signer>;
}

#[typetag::serde(tag = "type")]
pub trait Verifier: std::fmt::Debug + Async {
    fn verify(&self, msg: &Bytes, sig: &Bytes) -> Result<(), SignerError>;

    fn get_id(&self) -> KeyPairID;

    fn clone_self(&self) -> Box<dyn Verifier>;
}

impl Clone for Box<dyn Verifier> {
    fn clone(&self) -> Self {
        self.clone_self()
    }
}

impl PartialEq for Box<dyn Verifier>{
    fn eq(&self, other: &Self) -> bool {
        self.get_id() == other.get_id()
    }
}