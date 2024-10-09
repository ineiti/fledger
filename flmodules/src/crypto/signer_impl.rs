use bytes::Bytes;
use sha2::{Digest, Sha256};

use super::{
    entity::EntityID,
    signer::{Signature, SignerError, SignerImpl, VerifierImpl},
};

/**
 * Definition of ED25519 algorithm
 */

pub fn key_pair_ed25519_new() -> (Box<dyn SignerImpl>, Box<dyn VerifierImpl>) {
    let keypair = ed25519_compact::KeyPair::from_seed(ed25519_compact::Seed::default());
    let public = keypair.pk;
    (
        Box::new(SignerEd25519 { keypair }),
        Box::new(VerifierEd25519 { public }),
    )
}

pub fn key_pair_ed25519_from_bytes(
    b: &Bytes,
) -> Result<(Box<dyn SignerImpl>, Box<dyn VerifierImpl>), SignerError> {
    let keypair =
        ed25519_compact::KeyPair::from_slice(&b).map_err(|_| SignerError::SignerDecoding)?;
    let public = keypair.pk.clone();
    Ok((
        Box::new(SignerEd25519 { keypair }),
        Box::new(VerifierEd25519 { public }),
    ))
}

pub fn verifier_ed25519_from_bytes(b: &Bytes) -> Result<Box<dyn VerifierImpl>, SignerError> {
    let public =
        ed25519_compact::PublicKey::from_slice(b).map_err(|_| SignerError::VerifierDecoding)?;
    Ok(Box::new(VerifierEd25519 { public }))
}

pub struct SignerEd25519 {
    keypair: ed25519_compact::KeyPair,
}

impl SignerImpl for SignerEd25519 {
    fn to_bytes(&self) -> Bytes {
        self.keypair.to_vec().into()
    }

    fn sign(&self, msg: &Bytes) -> Signature {
        self.keypair
            .sk
            .sign(msg, Some(ed25519_compact::Noise::default()))
            .to_vec()
            .into()
    }
}
pub struct VerifierEd25519 {
    public: ed25519_compact::PublicKey,
}

impl VerifierImpl for VerifierEd25519 {
    fn to_bytes(&self) -> Bytes {
        self.public.to_vec().into()
    }

    fn verify(&self, msg: &Bytes, sig: &Signature) -> Result<(), SignerError> {
        let signature =
            ed25519_compact::Signature::from_slice(sig).map_err(|_| SignerError::SignatureType)?;
        self.public
            .verify(msg, &signature)
            .map_err(|_| SignerError::SignatureMessageMismatch)
    }

    fn clone(&self) -> Box<dyn VerifierImpl> {
        Box::new(VerifierEd25519 {
            public: self.public.clone(),
        })
    }

    fn get_id(&self) -> EntityID {
        let mut hasher = Sha256::new();
        hasher.update(self.public.to_vec());
        hasher.finalize().into()
    }
}

/**
 * Definition of ML-DSA-44 algorithm
 */

pub fn key_pair_ml_dsa44_new() -> (Box<dyn SignerImpl>, Box<dyn VerifierImpl>) {
    todo!()
}

pub fn key_pair_ml_dsa44_from_bytes(b: &Bytes) -> Result<(Box<dyn SignerImpl>, Box<dyn VerifierImpl>), SignerError> {
    todo!()
}

pub fn verifier_ml_dsa44_from_bytes(b: &Bytes) -> Result<Box<dyn VerifierImpl>, SignerError> {
    todo!()
}

pub struct SignerMlDSA44 {}

impl SignerImpl for SignerMlDSA44 {
    fn to_bytes(&self) -> Bytes {
        todo!()
    }

    fn sign(&self, msg: &Bytes) -> Signature {
        todo!()
    }
}
pub struct VerifierMlDSA44 {}

impl VerifierImpl for VerifierMlDSA44 {
    fn to_bytes(&self) -> Bytes {
        todo!()
    }

    fn verify(&self, msg: &Bytes, sig: &Signature) -> Result<(), SignerError> {
        todo!()
    }

    fn clone(&self) -> Box<dyn VerifierImpl> {
        todo!()
    }

    fn get_id(&self) -> EntityID {
        todo!()
    }
}

/**
 * Definition of ML-DSA-65 algorithm
 */

pub fn key_pair_ml_dsa65_new() -> (Box<dyn SignerImpl>, Box<dyn VerifierImpl>) {
    todo!()
}

pub fn key_pair_ml_dsa65_from_bytes(b: &Bytes) -> Result<(Box<dyn SignerImpl>, Box<dyn VerifierImpl>), SignerError> {
    todo!()
}

pub fn verifier_ml_dsa65_from_bytes(b: &Bytes) -> Result<Box<dyn VerifierImpl>, SignerError> {
    todo!()
}


pub struct SignerMlDSA65 {}

impl SignerImpl for SignerMlDSA65 {
    fn to_bytes(&self) -> Bytes {
        todo!()
    }

    fn sign(&self, msg: &Bytes) -> Signature {
        todo!()
    }
}
pub struct VerifierMlDSA65 {}

impl VerifierImpl for VerifierMlDSA65 {
    fn to_bytes(&self) -> Bytes {
        todo!()
    }

    fn verify(&self, msg: &Bytes, sig: &Signature) -> Result<(), SignerError> {
        todo!()
    }

    fn clone(&self) -> Box<dyn VerifierImpl> {
        todo!()
    }

    fn get_id(&self) -> EntityID {
        todo!()
    }
}

/**
 * Definition of ML-DSA-87 algorithm
 */

pub fn key_pair_ml_dsa87_new() -> (Box<dyn SignerImpl>, Box<dyn VerifierImpl>) {
    todo!()
}

pub fn key_pair_ml_dsa87_from_bytes(b: &Bytes) -> Result<(Box<dyn SignerImpl>, Box<dyn VerifierImpl>), SignerError> {
    todo!()
}

pub fn verifier_ml_dsa87_from_bytes(b: &Bytes) -> Result<Box<dyn VerifierImpl>, SignerError> {
    todo!()
}


pub struct SignerMlDSA87 {}

impl SignerImpl for SignerMlDSA87 {
    fn to_bytes(&self) -> Bytes {
        todo!()
    }

    fn sign(&self, msg: &Bytes) -> Signature {
        todo!()
    }
}
pub struct VerifierMlDSA87 {}

impl VerifierImpl for VerifierMlDSA87 {
    fn to_bytes(&self) -> Bytes {
        todo!()
    }

    fn verify(&self, msg: &Bytes, sig: &Signature) -> Result<(), SignerError> {
        todo!()
    }

    fn clone(&self) -> Box<dyn VerifierImpl> {
        todo!()
    }

    fn get_id(&self) -> EntityID {
        todo!()
    }
}
