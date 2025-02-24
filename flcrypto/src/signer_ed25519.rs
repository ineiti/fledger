use bytes::Bytes;
use ed25519_dalek::ed25519::signature::SignerMut;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

use super::signer::{KeyPairID, Signature, Signer, SignerError, Verifier};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SignerEd25519 {
    keypair: ed25519_dalek::SigningKey,
}

impl SignerEd25519 {
    pub fn new() -> Signer {
        Signer::Ed25519(Self {
            keypair: ed25519_dalek::SigningKey::generate(&mut OsRng),
        })
    }

    pub fn sign(&self, msg: &Bytes) -> Result<Signature, SignerError> {
        Ok(self.keypair.clone().sign(msg).to_bytes().to_vec().into())
    }

    pub fn verifier(&self) -> Verifier {
        Verifier::Ed25519(VerifierEd25519 {
            verifier: self.keypair.verifying_key(),
        })
    }

    pub fn get_id(&self) -> KeyPairID {
        (*self.verifier().get_id()).into()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct VerifierEd25519 {
    verifier: ed25519_dalek::VerifyingKey,
}

impl VerifierEd25519 {
    pub fn verify(&self, msg: &Bytes, sig: &Bytes) -> Result<(), SignerError> {
        let s: ed25519_dalek::Signature =
            TryInto::<[u8; 64]>::try_into(sig.as_ref()).unwrap().into();
        match self.verifier.verify_strict(msg, &s) {
            Ok(_) => Ok(()),
            Err(_) => Err(SignerError::SignatureMessageMismatch),
        }
    }

    pub fn get_id(&self) -> KeyPairID {
        KeyPairID::hash_domain_parts(
            "ican.re.fledg.flmodules.crypto.verifier_ed25519",
            &[self.verifier.as_bytes()],
        )
    }
}

#[cfg(test)]
mod test {
    use std::error::Error;

    use super::*;

    #[test]
    fn test_ed25519() -> Result<(), Box<dyn Error>> {
        let signer = SignerEd25519::new();
        let verifier = signer.verifier();
        let msg1 = Bytes::from("message1");
        let msg2 = Bytes::from("message2");

        let sig1 = signer.sign(&msg1)?;
        let sig2 = signer.sign(&msg2)?;

        verifier.verify(&msg1, &sig1)?;
        verifier.verify(&msg2, &sig2)?;

        assert!(verifier.verify(&msg1, &sig2).is_err());
        assert!(verifier.verify(&msg2, &sig1).is_err());

        Ok(())
    }
}
