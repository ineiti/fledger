use bytes::Bytes;

use crate::crypto::signer::{Signature, SignerError};

use super::access::ID;

#[typetag::serde(tag = "type")]
pub trait Signer {
    fn sign(&self, msg: &Bytes) -> Result<Signature, SignerError>;

    fn verifier(&self) -> Box<dyn Verifier>;

    fn get_id(&self) -> ID;

    fn clone(&self) -> Box<dyn Signer>;
}

#[typetag::serde(tag = "type")]
pub trait Verifier {
    fn verify(&self, msg: &Bytes, sig: &Bytes) -> Result<(), SignerError>;

    fn get_id(&self) -> ID;

    fn clone(&self) -> Box<dyn Verifier>;
}
