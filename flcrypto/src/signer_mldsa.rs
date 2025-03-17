use std::fmt::Debug;

use bytes::Bytes;
use pqcrypto_mldsa::{mldsa44, mldsa65, mldsa87};
use pqcrypto_traits::sign::DetachedSignature;
use serde::{Deserialize, Serialize};

use crate::{
    signer::{KeyPairID, Signature, Signer, SignerError, SignerTrait, Verifier, VerifierTrait},
    tofrombytes::ToFromBytes,
};

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub enum SignerMlDSA {
    MlDSA44(mldsa44::PublicKey, mldsa44::SecretKey),
    MlDSA65(mldsa65::PublicKey, mldsa65::SecretKey),
    MlDSA87(mldsa87::PublicKey, mldsa87::SecretKey),
}

impl SignerTrait for SignerMlDSA {
    fn sign(&self, msg: &Bytes) -> anyhow::Result<Signature> {
        Ok(match self {
            SignerMlDSA::MlDSA44(_, secret_key) => {
                Bytes::copy_from_slice(mldsa44::detached_sign(msg, secret_key).as_bytes())
            }
            SignerMlDSA::MlDSA65(_, secret_key) => {
                Bytes::copy_from_slice(mldsa65::detached_sign(msg, secret_key).as_bytes())
            }
            SignerMlDSA::MlDSA87(_, secret_key) => {
                Bytes::copy_from_slice(mldsa87::detached_sign(msg, secret_key).as_bytes())
            }
        })
    }

    fn verifier(&self) -> Verifier {
        Verifier::MlDSA(match self {
            SignerMlDSA::MlDSA44(public_key, _) => VerifierMlDSA::MlDSA44(public_key.clone()),
            SignerMlDSA::MlDSA65(public_key, _) => VerifierMlDSA::MlDSA65(public_key.clone()),
            SignerMlDSA::MlDSA87(public_key, _) => VerifierMlDSA::MlDSA87(public_key.clone()),
        })
    }

    fn get_id(&self) -> KeyPairID {
        self.verifier().get_id()
    }
}

impl SignerMlDSA {
    pub fn new44() -> Signer {
        let (pk, sk) = mldsa44::keypair();
        Signer::MlDSA(Self::MlDSA44(pk, sk))
    }

    pub fn new65() -> Signer {
        let (pk, sk) = mldsa65::keypair();
        Signer::MlDSA(Self::MlDSA65(pk, sk))
    }

    pub fn new87() -> Signer {
        let (pk, sk) = mldsa87::keypair();
        Signer::MlDSA(Self::MlDSA87(pk, sk))
    }
}

impl Debug for SignerMlDSA {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MlDSA44(_, _) => f.debug_tuple("MlDSA44").finish(),
            Self::MlDSA65(_, _) => f.debug_tuple("MlDSA65").finish(),
            Self::MlDSA87(_, _) => f.debug_tuple("MlDSA87").finish(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub enum VerifierMlDSA {
    MlDSA44(mldsa44::PublicKey),
    MlDSA65(mldsa65::PublicKey),
    MlDSA87(mldsa87::PublicKey),
}

impl VerifierTrait for VerifierMlDSA {
    fn verify(&self, msg: &Bytes, sig: &Bytes) -> anyhow::Result<()> {
        match self {
            VerifierMlDSA::MlDSA44(public_key) => {
                let ds = mldsa44::DetachedSignature::from_bytes(sig)
                    .map_err(|e| SignerError::PQCrypto(format!("Signature error {e:?}")))?;
                mldsa44::verify_detached_signature(&ds, msg, public_key)
                    .map_err(|_| SignerError::SignatureMessageMismatch.into())
            }
            VerifierMlDSA::MlDSA65(public_key) => {
                let ds = mldsa65::DetachedSignature::from_bytes(sig)
                    .map_err(|e| SignerError::PQCrypto(format!("Signature error {e:?}")))?;
                mldsa65::verify_detached_signature(&ds, msg, public_key)
                    .map_err(|_| SignerError::SignatureMessageMismatch.into())
            }
            VerifierMlDSA::MlDSA87(public_key) => {
                let ds = mldsa87::DetachedSignature::from_bytes(sig)
                    .map_err(|e| SignerError::PQCrypto(format!("Signature error {e:?}")))?;
                mldsa87::verify_detached_signature(&ds, msg, public_key)
                    .map_err(|_| SignerError::SignatureMessageMismatch.into())
            }
        }
    }

    fn get_id(&self) -> KeyPairID {
        KeyPairID::hash_domain_parts(&format!("{:?}", self), &[&self.to_rmp_bytes()])
    }
}

impl Debug for VerifierMlDSA {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MlDSA44(_) => f.debug_tuple("MlDSA44").finish(),
            Self::MlDSA65(_) => f.debug_tuple("MlDSA65").finish(),
            Self::MlDSA87(_) => f.debug_tuple("MlDSA87").finish(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_sign() -> anyhow::Result<()> {
        sign_verify(SignerMlDSA::new44())?;
        sign_verify(SignerMlDSA::new65())?;
        sign_verify(SignerMlDSA::new87())?;
        Ok(())
    }

    fn sign_verify(signer: Signer) -> anyhow::Result<()> {
        let msg = Bytes::from("123");
        let sig = signer.sign(&msg)?;
        let ver = signer.verifier();
        ver.verify(&msg, &sig)
    }
}
