use bytes::Bytes;
use serde::{ser::SerializeStruct, Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use super::{
    entity::EntityID,
    signer_impl::{
        key_pair_ed25519_from_bytes, key_pair_ed25519_new, key_pair_ml_dsa44_from_bytes,
        key_pair_ml_dsa44_new, key_pair_ml_dsa65_from_bytes, key_pair_ml_dsa65_new,
        key_pair_ml_dsa87_from_bytes, key_pair_ml_dsa87_new, verifier_ed25519_from_bytes,
        verifier_ml_dsa44_from_bytes, verifier_ml_dsa65_from_bytes, verifier_ml_dsa87_from_bytes,
    },
};

#[derive(Debug, Error)]
pub enum SignerError {
    #[error("Wrong type of signature")]
    SignatureType,
    #[error("Signature doesn't match message")]
    SignatureMessageMismatch,
    #[error("Decoding error in signer")]
    SignerDecoding,
    #[error("Decoding error in verifier")]
    VerifierDecoding,
}

pub struct Signer {
    sig: SignatureType,
    signer: Box<dyn SignerImpl>,
    verifier: Box<dyn VerifierImpl>,
}

impl Signer {
    pub fn new(sig: SignatureType) -> Self {
        let (signer, verifier) = match sig {
            SignatureType::Ed25519 => key_pair_ed25519_new(),
            SignatureType::MlDsa44 => key_pair_ml_dsa44_new(),
            SignatureType::MlDsa65 => key_pair_ml_dsa65_new(),
            SignatureType::MlDsa87 => key_pair_ml_dsa87_new(),
        };
        Self {
            sig,
            signer,
            verifier,
        }
    }

    pub fn sign(&self, msg: &Bytes) -> Result<Signature, SignerError> {
        self.signer.sign(msg)
    }

    pub fn verifier(&self) -> Verifier {
        Verifier {
            sig: self.sig,
            verifier: self.verifier.clone(),
        }
    }

    pub fn get_id(&self) -> EntityID {
        self.verifier.get_id()
    }
}

pub struct Verifier {
    sig: SignatureType,
    verifier: Box<dyn VerifierImpl>,
}

impl Verifier {
    pub fn verify(&self, msg: &Bytes, sig: &Bytes) -> Result<(), SignerError> {
        self.verifier.verify(msg, sig)
    }

    pub fn get_id(&self) -> EntityID {
        self.verifier.get_id()
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, strum_macros::Display)]
pub enum SignatureType {
    Ed25519,
    MlDsa44,
    MlDsa65,
    MlDsa87,
}

pub type Signature = Bytes;

impl Serialize for Verifier {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let bytes = self.verifier.to_bytes();
        let mut state = serializer.serialize_struct("Verifier", 2)?;
        state.serialize_field("sig", &self.sig.to_string())?;
        state.serialize_field("verifier", &bytes)?;
        state.end()
    }
}

impl Clone for Verifier {
    fn clone(&self) -> Self {
        verifier_from_bytes(self.sig, &self.verifier.to_bytes()).unwrap()
    }
}

impl Serialize for Signer {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let bytes = self.signer.to_bytes();
        let mut state = serializer.serialize_struct("Signer", 2)?;
        state.serialize_field("sig", &self.sig.to_string())?;
        state.serialize_field("signer", &bytes)?;
        state.end()
    }
}
impl Clone for Signer {
    fn clone(&self) -> Self {
        signer_from_bytes(self.sig, &self.signer.to_bytes()).unwrap()
    }
}

impl<'de> Deserialize<'de> for Verifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct VerifierFields {
            sig: String,
            verifier: Bytes,
        }

        let fields = VerifierFields::deserialize(deserializer)?;

        let sig = match fields.sig.as_str() {
            "Ed25519" => SignatureType::Ed25519,
            "MlDsa44" => SignatureType::MlDsa44,
            "MlDsa65" => SignatureType::MlDsa65,
            "MlDsa87" => SignatureType::MlDsa87,
            _ => return Err(serde::de::Error::custom("Invalid signature type")),
        };

        verifier_from_bytes(sig, &fields.verifier)
            .map_err(|e| serde::de::Error::custom(e.to_string()))
    }
}

impl<'de> Deserialize<'de> for Signer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct VerifierFields {
            sig: String,
            signer: Bytes,
        }

        let fields = VerifierFields::deserialize(deserializer)?;

        let sig = match fields.sig.as_str() {
            "Ed25519" => SignatureType::Ed25519,
            "MlDsa44" => SignatureType::MlDsa44,
            "MlDsa65" => SignatureType::MlDsa65,
            "MlDsa87" => SignatureType::MlDsa87,
            _ => return Err(serde::de::Error::custom("Invalid signature type")),
        };

        signer_from_bytes(sig, &fields.signer).map_err(|e| serde::de::Error::custom(e.to_string()))
    }
}

pub trait VerifierImpl {
    fn to_bytes(&self) -> Bytes;

    fn verify(&self, msg: &Bytes, sig: &Signature) -> Result<(), SignerError>;

    fn clone(&self) -> Box<dyn VerifierImpl>;

    fn get_id(&self) -> EntityID;
}

pub trait SignerImpl {
    fn to_bytes(&self) -> Bytes;

    fn sign(&self, msg: &Bytes) -> Result<Signature, SignerError>;
}

pub(crate) fn signer_from_bytes(st: SignatureType, b: &Bytes) -> Result<Signer, SignerError> {
    let (signer, verifier) = match st {
        SignatureType::Ed25519 => key_pair_ed25519_from_bytes(&b)?,
        SignatureType::MlDsa44 => key_pair_ml_dsa44_from_bytes(&b)?,
        SignatureType::MlDsa65 => key_pair_ml_dsa65_from_bytes(&b)?,
        SignatureType::MlDsa87 => key_pair_ml_dsa87_from_bytes(&b)?,
    };
    Ok(Signer {
        sig: st,
        signer,
        verifier,
    })
}

pub(crate) fn verifier_from_bytes(st: SignatureType, b: &Bytes) -> Result<Verifier, SignerError> {
    let verifier = match st {
        SignatureType::Ed25519 => verifier_ed25519_from_bytes(b)?,
        SignatureType::MlDsa44 => verifier_ml_dsa44_from_bytes(b)?,
        SignatureType::MlDsa65 => verifier_ml_dsa65_from_bytes(b)?,
        SignatureType::MlDsa87 => verifier_ml_dsa87_from_bytes(b)?,
    };
    Ok(Verifier { sig: st, verifier })
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_sigs() -> Result<(), SignerError> {
        test_sig(SignatureType::Ed25519)?;
        test_sig(SignatureType::MlDsa44)?;
        test_sig(SignatureType::MlDsa65)?;
        test_sig(SignatureType::MlDsa87)?;
        Ok(())
    }

    fn test_sig(sig: SignatureType) -> Result<(), SignerError> {
        let signer = Signer::new(sig);
        let msg1 = Bytes::from("value1");
        let signature1 = signer.sign(&msg1)?;
        let msg2 = Bytes::from("value2");
        let signature2 = signer.sign(&msg2)?;
        signer.verifier().verify(&msg1, &signature1)?;
        signer.verifier().verify(&msg2, &signature2)?;

        assert!(signer.verifier().verify(&msg1, &signature2).is_err());
        assert!(signer.verifier().verify(&msg2, &signature1).is_err());
        Ok(())
    }
}
