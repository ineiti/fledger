use bytes::Bytes;
use ed25519_dalek::ed25519::signature::SignerMut;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

use super::signer::{Signature, Signer, SignerError, KeyPairID, Verifier};

#[derive(Serialize, Deserialize, Debug)]
pub struct SignerEd25519 {
    keypair: ed25519_dalek::SigningKey,
}

impl SignerEd25519 {
    pub fn new_box() -> Box<dyn Signer> {
        Box::new(SignerEd25519 {
            keypair: ed25519_dalek::SigningKey::generate(&mut OsRng),
        })
    }
}

#[typetag::serde]
impl Signer for SignerEd25519 {
    fn sign(&self, msg: &Bytes) -> Result<Signature, SignerError> {
        Ok(self.keypair.clone().sign(msg).to_bytes().to_vec().into())
    }

    fn verifier(&self) -> Box<dyn Verifier> {
        Box::new(VerifierEd25519 {
            verifier: self.keypair.verifying_key(),
        })
    }

    fn get_id(&self) -> KeyPairID {
        (*self.verifier().get_id()).into()
    }

    fn clone(&self) -> Box<dyn Signer> {
        Box::new(Self {
            keypair: self.keypair.clone(),
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VerifierEd25519 {
    verifier: ed25519_dalek::VerifyingKey,
}

#[typetag::serde]
impl Verifier for VerifierEd25519 {
    fn verify(&self, msg: &Bytes, sig: &Bytes) -> Result<(), SignerError> {
        let s: ed25519_dalek::Signature =
            TryInto::<[u8; 64]>::try_into(sig.as_ref()).unwrap().into();
        match self.verifier.verify_strict(msg, &s) {
            Ok(_) => Ok(()),
            Err(_) => Err(SignerError::SignatureMessageMismatch),
        }
    }

    fn get_id(&self) -> KeyPairID {
        KeyPairID::hash_domain_parts(
            "ican.re.fledg.flmodules.crypto.verifier_ed25519",
            &[self.verifier.as_bytes()],
        )
    }

    fn clone_self(&self) -> Box<dyn Verifier> {
        Box::new(VerifierEd25519 {
            verifier: self.verifier.clone(),
        })
    }
}

#[cfg(test)]
mod test {
    use std::error::Error;

    use super::*;

    #[test]
    fn test_ed25519() -> Result<(), Box<dyn Error>> {
        let signer = SignerEd25519::new_box();
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
