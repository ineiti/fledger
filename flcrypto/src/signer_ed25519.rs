use bytes::Bytes;
use ed25519_dalek::ed25519::signature::SignerMut;
use rand::rngs::OsRng;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use base64::prelude::*;

use crate::signer::{SignerTrait, VerifierTrait};

use super::signer::{KeyPairID, Signature, Signer, SignerError, Verifier};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SignerEd25519 {
    keypair: ed25519_dalek::SigningKey,
}

impl SignerTrait for SignerEd25519 {
    fn sign(&self, msg: &Bytes) -> anyhow::Result<Signature> {
        Ok(self.keypair.clone().sign(msg).to_bytes().to_vec().into())
    }

    fn verifier(&self) -> Verifier {
        Verifier::Ed25519(VerifierEd25519 {
            verifier: self.keypair.verifying_key(),
        })
    }

    fn get_id(&self) -> KeyPairID {
        (*self.verifier().get_id()).into()
    }
}

impl SignerEd25519 {
    pub fn new() -> Signer {
        Signer::Ed25519(Self {
            keypair: ed25519_dalek::SigningKey::generate(&mut OsRng),
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct VerifierEd25519 {
    #[serde(
        serialize_with = "serialize_key_as_base64",
        deserialize_with = "deserialize_key_from_base64"
    )]
    verifier: ed25519_dalek::VerifyingKey,
}

fn serialize_key_as_base64<S>(
    key: &ed25519_dalek::VerifyingKey,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let base64_str = BASE64_STANDARD.encode(key.to_bytes());
    serializer.serialize_str(&base64_str)
}

fn deserialize_key_from_base64<'de, D>(
    deserializer: D,
) -> Result<ed25519_dalek::VerifyingKey, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    let base64_str: String = Deserialize::deserialize(deserializer)?;
    let bytes = BASE64_STANDARD
        .decode(base64_str.as_bytes())
        .map_err(|e| D::Error::custom(format!("Base64 decode error: {}", e)))?;
    ed25519_dalek::VerifyingKey::from_bytes(
        &bytes
            .try_into()
            .map_err(|_| D::Error::custom("Invalid key length"))?,
    )
    .map_err(|e| D::Error::custom(format!("Invalid key: {}", e)))
}

impl VerifierTrait for VerifierEd25519 {
    fn verify(&self, msg: &Bytes, sig: &Signature) -> anyhow::Result<()> {
        let s: ed25519_dalek::Signature =
            TryInto::<[u8; 64]>::try_into(sig.bytes().as_ref()).unwrap().into();
        match self.verifier.verify_strict(msg, &s) {
            Ok(_) => Ok(()),
            Err(_) => Err(SignerError::SignatureMessageMismatch.into()),
        }
    }

    fn get_id(&self) -> KeyPairID {
        KeyPairID::hash_domain_parts(
            "ican.re.fledg.flmodules.crypto.verifier_ed25519",
            &[self.verifier.as_bytes()],
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_ed25519() -> anyhow::Result<()> {
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

    #[test]
    fn serializing() -> anyhow::Result<()> {
        let signer = SignerEd25519::new();
        let ver = signer.verifier();
        println!("{}", serde_yaml::to_string(&ver)?);
        Ok(())
    }
}
