use bytes::Bytes;
use flarch::nodeids::U256;
use flarch_macro::AsU256;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type Signature = Bytes;

#[derive(Debug, Error)]
pub enum SignerError {
    #[error("Signature doesn't match message")]
    SignatureMessageMismatch,
}

#[derive(AsU256, Serialize, Deserialize, Clone)]
pub struct VerifierID(U256);

#[derive(AsU256, Serialize, Deserialize, Clone)]
pub struct SignerID(U256);

#[typetag::serde(tag = "type")]
pub trait Signer: std::fmt::Debug {
    fn sign(&self, msg: &Bytes) -> Result<Signature, SignerError>;

    fn verifier(&self) -> Box<dyn Verifier>;

    fn get_id(&self) -> SignerID;

    fn clone(&self) -> Box<dyn Signer>;
}

#[typetag::serde(tag = "type")]
pub trait Verifier: std::fmt::Debug {
    fn verify(&self, msg: &Bytes, sig: &Bytes) -> Result<(), SignerError>;

    fn get_id(&self) -> VerifierID;

    fn clone(&self) -> Box<dyn Verifier>;
}
