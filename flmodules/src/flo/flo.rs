use std::any::type_name;

use bytes::Bytes;
use flarch::nodeids::U256;
use flmacro::{AsU256, VersionedSerde};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_with::{hex::Hex, serde_as};
use thiserror::Error;

use flcrypto::{
    access::{Condition, ConditionLink, ConditionSignature, VersionSpec},
    signer::{Signer, SignerError},
    tofrombytes::{TFBError, ToFromBytes},
};

use crate::dht_storage::core::{Cuckoo, FloConfig};

use super::realm::{GlobalID, Realm, RealmID};

#[derive(Debug, Error)]
pub enum FloError {
    #[error("{0}")]
    DeserializationTFB(#[from] TFBError),
    #[error("Couldn't deserialize {0}: '{1:?}'")]
    Deserialization(String, String),
    #[error("Couldn't serialize: {0}")]
    Serialization(String),
    #[error("RMP serialization error {0}")]
    RmpSerialization(#[from] rmp_serde::encode::Error),
    #[error("Expected content to be '{0:?}', but was '{1:?}")]
    WrongContent(String, String),
    #[error("SignerError: {0}")]
    SignerError(#[from] SignerError),
}

#[derive(AsU256, Deserialize, Serialize, Clone, PartialEq, Eq, Hash)]
pub struct FloID(U256);

/// Flo defines the following actions:
/// - ican.re.fledg.flmodules.flo.flo
///   - .update_data - update the data
///   - .update_ace - update the Version<AceID>
#[serde_as]
#[derive(VersionedSerde, Debug, Clone, PartialEq)]
pub struct Flo {
    // In which realm this Flo resides.
    realm: RealmID,
    // What type this Flo is - should be a generic mime-type
    flo_type: String,
    // Does it allow storing of nearby Cuckoos?
    flo_config: FloConfig,
    // The current data of the Flo
    #[serde_as(as = "Hex")]
    data: Bytes,
    // The current version of the data, which increases independently
    // from the cond version.
    // When a new cond version is added, the data_version stays the same.
    data_version: u32,
    // The data signature is verifiable by the latest cond.
    data_signature: ConditionSignature,
    // Allows for a random ID even for two Flos with the same data.
    // Because I thought that ed25519 signatures have a random nonce, when in
    // fact the implementation of ed25519-dalek uses a nonce derived from the
    // message, probably to avoid nonce-reuse...
    nonce: U256,
    // The first condition of this Flo, together with a signature on itself.
    cond_genesis: CondVersion,
    // Every new version of the cond is signed by the signers of the previous
    // cond.
    cond_update: Vec<CondVersion>,
}

/// One version of the cond.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CondVersion {
    // The new cond of this step.
    cond: ConditionLink,
    // The signature also holds the verifiers to verify the signature.
    signatures: ConditionSignature,
}

/// A convenience method to handle a type T together with its
/// Flo-representation of all the history and ACE.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FloWrapper<T> {
    flo: Flo,
    object: T,
}

pub struct UpdateDataSign<T: Serialize + DeserializeOwned + Clone> {
    digest: U256,
    update: T,
    signature: ConditionSignature,
}

pub struct UpdateCondSign {
    cond: ConditionLink,
    digest_data: U256,
    digest_cond: U256,
    signature_data: ConditionSignature,
    signature_cond: ConditionSignature,
}

impl Flo {
    pub fn new<T: Serialize + DeserializeOwned>(
        realm: RealmID,
        cond: ConditionLink,
        data_cache: &T,
        flo_config: FloConfig,
        nonce: U256,
        cond_signature: ConditionSignature,
        data_signature: ConditionSignature,
    ) -> anyhow::Result<Self> {
        let data = data_cache.to_rmp_bytes();
        let flo_type = type_name::<T>().to_string();
        Ok(Self {
            realm,
            flo_type,
            flo_config,
            data,
            data_version: 0,
            data_signature,
            cond_update: vec![],
            nonce,
            cond_genesis: CondVersion {
                cond,
                signatures: cond_signature,
            },
        })
    }

    pub fn new_signer<T: Serialize + DeserializeOwned>(
        realm: RealmID,
        cond: Condition,
        data_cache: &T,
        flo_config: FloConfig,
        signers: &[&Signer],
    ) -> anyhow::Result<Self> {
        let data = data_cache.to_rmp_bytes();
        let flo_type = type_name::<T>().to_string();
        let mut flo = Flo {
            realm,
            flo_type,
            flo_config,
            data,
            data_version: 0,
            data_signature: ConditionSignature::empty(),
            nonce: U256::rnd(),
            cond_genesis: CondVersion {
                cond: cond.to_link(),
                signatures: ConditionSignature::new(cond.clone()),
            },
            cond_update: vec![],
        };
        let digest = flo.digest_cond_genesis();
        for sig in signers {
            flo.cond_genesis.signatures.sign_digest(sig, &digest)?;
        }
        flo.sign_data(cond, signers)?;

        Ok(flo)
    }

    pub fn new_realm(cond: Condition, realm: &Realm, signers: &[&Signer]) -> anyhow::Result<Self> {
        let mut f = Self::new_signer(RealmID::zero(), cond, realm, FloConfig::default(), signers)?;
        f.realm = (*f.flo_id()).into();
        Ok(f)
    }

    pub fn data(&self) -> &Bytes {
        &self.data
    }

    pub fn cond(&self) -> &ConditionLink {
        &self.latest_cond_version().cond
    }

    pub fn version(&self) -> u32 {
        self.data_version
    }

    pub fn flo_id(&self) -> FloID {
        Self::calc_flo_id(
            self.flo_config
                .force_id
                .as_ref()
                .unwrap_or(&self.digest_cond_genesis()),
        )
    }

    pub fn global_id(&self) -> GlobalID {
        GlobalID::new(self.realm.clone(), self.flo_id())
    }

    pub fn realm_id(&self) -> RealmID {
        self.realm.clone()
    }

    pub fn flo_type(&self) -> String {
        self.flo_type.clone()
    }

    pub fn flo_config(&self) -> &FloConfig {
        &self.flo_config
    }

    pub fn calc_flo_id(hash: &U256) -> FloID {
        FloID::hash_domain_parts("ican.re.fledg.flmodules.flo.Flo", &[&hash.bytes()])
    }

    /// Returns an [UpdateDataSign] to collect all signatures necessary
    /// for updating the data part of this Flo.
    pub fn start_sign_data<T: Serialize + DeserializeOwned + Clone>(
        &self,
        cond: Condition,
        update: T,
    ) -> anyhow::Result<UpdateDataSign<T>> {
        let mut next_version = self.clone();
        next_version.data = update.to_rmp_bytes();
        next_version.data_version += 1;
        return Ok(UpdateDataSign {
            digest: next_version.digest_data(),
            update,
            signature: ConditionSignature::new(cond),
        });
    }

    /// Updates the data of this Flow and returns a new Flo.
    pub fn update_data<T: Serialize + DeserializeOwned + Clone>(
        &self,
        update: UpdateDataSign<T>,
    ) -> anyhow::Result<Flo> {
        if !update.signature.verify_digest(&update.digest)? {
            return Err(SignerError::SignatureMessageMismatch.into());
        }

        let mut next_version = self.clone();
        next_version.data = update.update.to_rmp_bytes();
        next_version.data_signature = update.signature;
        next_version.data_version += 1;
        if next_version.digest_data() != update.digest {
            return Err(SignerError::SignatureMessageMismatch.into());
        }
        Ok(next_version)
    }

    /// Updates the data of this Flo, given the [Condition] and the new
    /// data [T].
    /// The signers will all be used to create a signature.
    /// It returns the new Flo with the updated data and signature.
    pub fn update_data_signers<T: Serialize + DeserializeOwned + Clone>(
        &self,
        cond: Condition,
        update: T,
        signers: &[&Signer],
    ) -> anyhow::Result<Self> {
        let mut up_sig = self.start_sign_data(cond, update)?;
        up_sig.signature.signers_digest(signers, &up_sig.digest)?;
        self.update_data(up_sig)
    }

    /// Returns an [UpdateCondSign] to collect the signatures necessary
    /// to update the condition of this Flo.
    pub fn start_sign_cond(
        &self,
        cond_old: Condition,
        cond_new: Condition,
    ) -> anyhow::Result<UpdateCondSign> {
        let mut next_version = self.clone();
        next_version.cond_update.push(CondVersion {
            cond: cond_new.to_link(),
            signatures: ConditionSignature::new(cond_old.clone()),
        });
        return Ok(UpdateCondSign {
            cond: cond_new.to_link(),
            digest_cond: Self::digest_cond_update(
                &self.flo_id(),
                next_version.cond_update.len() as u32,
                &cond_new.to_link(),
            ),
            digest_data: next_version.digest_data(),
            signature_data: ConditionSignature::new(cond_new),
            signature_cond: ConditionSignature::new(cond_old),
        });
    }

    /// Updates the signing condition and returns the new Flo.
    /// TODO: check that new signature is valid.
    pub fn update_cond(&self, update: UpdateCondSign) -> anyhow::Result<Flo> {
        let mut next_version = self.clone();
        next_version.cond_update.push(CondVersion {
            cond: update.cond,
            signatures: update.signature_cond,
        });
        next_version.data_signature = update.signature_data;
        Ok(next_version)
    }

    /// Updates signs a new condition by the previous signers, and returns
    /// the new Flo.
    pub fn update_cond_signers(
        &self,
        cond_old: Condition,
        cond_new: Condition,
        signers_old: &[&Signer],
        signers_new: &[&Signer],
    ) -> anyhow::Result<Flo> {
        let mut update = self.start_sign_cond(cond_old, cond_new)?;
        update
            .signature_cond
            .signers_digest(signers_old, &update.digest_cond)?;
        update
            .signature_data
            .signers_digest(signers_new, &update.digest_data)?;

        self.update_cond(update)
    }

    /// Returns true if the signature is correct, else it returns false or a
    /// more specific error why the signature is wrong.
    pub fn verify(&self) -> anyhow::Result<bool> {
        // Verify the genesis signature is correct
        if !self
            .cond_genesis
            .signatures
            .verify_digest(&self.digest_cond_genesis())?
            || !self
                .cond_genesis
                .signatures
                .condition
                .equal_link(&self.cond_genesis.cond)
        {
            return Err(SignerError::SignatureMessageMismatch.into());
        }

        // Verify all cond updates are correct
        let mut prev_cond = &self.cond_genesis.cond;
        for (ver, cv) in self.cond_update.iter().enumerate() {
            let digest = &Self::digest_cond_update(&self.flo_id(), (ver + 1) as u32, &cv.cond);
            if !cv.signatures.condition.equal_link(prev_cond)
                || !cv.signatures.verify_digest(digest)?
            {
                return Err(SignerError::SignatureMessageMismatch.into());
            }
            prev_cond = &cv.cond;
        }

        // Finally verify the latest data is correct
        if !self.data_signature.condition.equal_link(prev_cond)
            || !self.data_signature.verify_digest(&self.digest_data())?
        {
            return Err(SignerError::SignatureMessageMismatch.into());
        }

        Ok(true)
    }

    fn latest_cond_version(&self) -> &CondVersion {
        self.cond_update.last().unwrap_or(&self.cond_genesis)
    }

    fn sign_data(&mut self, cond: Condition, signers: &[&Signer]) -> anyhow::Result<()> {
        let digest = self.digest_data();
        self.data_signature = ConditionSignature::new(cond);
        for signer in signers {
            self.data_signature.sign_digest(signer, &digest)?;
        }
        Ok(())
    }

    fn digest_cond_genesis(&self) -> U256 {
        U256::hash_domain_parts(
            "flmodules::flo::cond_genesis",
            &[
                self.flo_type.as_ref(),
                &self.flo_config.to_rmp_bytes(),
                &self.cond_genesis.cond.to_rmp_bytes(),
                &self.nonce.bytes(),
            ],
        )
    }

    fn digest_cond_update(id: &FloID, cond_version: u32, cond: &ConditionLink) -> U256 {
        U256::hash_domain_parts(
            "flmodules::flo::cond_update",
            &[
                &id.bytes(),
                &cond_version.to_le_bytes(),
                &cond.to_rmp_bytes(),
            ],
        )
    }

    fn digest_data(&self) -> U256 {
        U256::hash_domain_parts(
            "flmodules::flo::data",
            &[
                &self.flo_id().bytes(),
                &self.data,
                &self.data_version.to_le_bytes(),
                &self.cond_update.len().to_le_bytes(),
            ],
        )
    }
}

impl<T: Serialize + DeserializeOwned + Clone> FloWrapper<T> {
    pub fn from_type(
        realm: RealmID,
        cond: Condition,
        object: T,
        signers: &[&Signer],
    ) -> anyhow::Result<Self> {
        Self::from_type_config(realm, cond, FloConfig::default(), object, signers)
    }

    pub fn from_type_cuckoo(
        realm: RealmID,
        cond: Condition,
        cuckoo: Cuckoo,
        object: T,
        signers: &[&Signer],
    ) -> anyhow::Result<Self> {
        Self::from_type_config(
            realm,
            cond,
            FloConfig {
                cuckoo,
                force_id: None,
            },
            object,
            signers,
        )
    }

    pub fn from_type_config(
        realm: RealmID,
        cond: Condition,
        config: FloConfig,
        object: T,
        signers: &[&Signer],
    ) -> anyhow::Result<Self> {
        let flo = Flo::new_signer(realm, cond, &object, config, signers)?;
        Ok(Self { flo, object })
    }

    pub fn from_force_id(realm: RealmID, object: T, force_id: U256) -> anyhow::Result<Self> {
        let flo = Flo::new_signer(
            realm,
            Condition::Fail,
            &object,
            FloConfig {
                cuckoo: Cuckoo::None,
                force_id: Some(force_id),
            },
            &[],
        )?;
        Ok(Self { flo, object })
    }

    pub fn cache(&self) -> &T {
        &self.object
    }

    pub fn cache_mut(&mut self) -> &mut T {
        &mut self.object
    }

    pub fn flo(&self) -> &Flo {
        &self.flo
    }

    pub fn cond(&self) -> &ConditionLink {
        self.flo.cond()
    }

    pub fn data(&self) -> &Bytes {
        &self.flo.data
    }

    pub fn version(&self) -> u32 {
        self.flo.data_version as u32
    }

    pub fn flo_id(&self) -> FloID {
        self.flo.flo_id()
    }

    pub fn global_id(&self) -> GlobalID {
        GlobalID::new(self.flo.realm.clone(), self.flo_id())
    }

    pub fn realm_id(&self) -> RealmID {
        self.flo.realm.clone()
    }

    pub fn edit(&self, edit: impl FnOnce(&mut T)) -> T {
        let mut e = self.cache().clone();
        edit(&mut e);
        e
    }

    pub fn hash_update(&self, update: &T) -> U256 {
        let new_hash = U256::hash_data(&update.to_rmp_bytes()).to_bytes();
        U256::hash_domain_parts("FloUpdate", &[&self.hash().to_bytes(), &new_hash])
    }

    pub fn start_sign_data(&self, cond: Condition, update: T) -> anyhow::Result<UpdateDataSign<T>> {
        self.flo.start_sign_data(cond, update)
    }

    pub fn start_sign_cond(
        &self,
        cond_old: Condition,
        cond_new: Condition,
    ) -> anyhow::Result<UpdateCondSign> {
        self.flo.start_sign_cond(cond_old, cond_new)
    }

    pub fn edit_data_signers(
        &self,
        cond: Condition,
        edit: impl FnOnce(&mut T),
        signers: &[&Signer],
    ) -> anyhow::Result<Self> {
        let update = self.edit(edit);
        let mut new_fw = self.clone();
        new_fw.flo = self
            .flo
            .update_data_signers(cond, update.clone(), signers)?;
        new_fw.object = update;
        Ok(new_fw)
    }

    pub fn update_cond_signers(
        &mut self,
        cond_old: Condition,
        cond_new: Condition,
        signers_old: &[&Signer],
        signers_new: &[&Signer],
    ) -> anyhow::Result<Flo> {
        self.flo
            .update_cond_signers(cond_old, cond_new, signers_old, signers_new)
    }

    pub fn update_data(&mut self, update: UpdateDataSign<T>) -> anyhow::Result<Self> {
        let mut new_fw = self.clone();
        new_fw.object = update.update.clone();
        new_fw.flo = self.flo.update_data(update)?;
        Ok(new_fw)
    }

    pub fn update_cond(&mut self, update: UpdateCondSign) -> anyhow::Result<Self> {
        let mut new_fw = self.clone();
        new_fw.flo = self.flo.update_cond(update)?;
        Ok(new_fw)
    }

    pub fn verify(&self) -> anyhow::Result<bool> {
        self.flo.verify()
    }

    pub fn fits_version<ID: Serialize + Clone + AsRef<[u8]> + PartialEq>(
        &self,
        ver: &VersionSpec<ID>,
    ) -> bool {
        ver.get_id().as_ref() == self.flo_id().as_ref() && ver.accepts(self.version())
    }

    pub fn hash(&self) -> U256 {
        U256::hash_domain_parts("FloWrapper", &[&self.flo.to_rmp_bytes()])
    }
}

impl<T: Serialize + DeserializeOwned> TryFrom<Flo> for FloWrapper<T> {
    type Error = anyhow::Error;

    fn try_from(flo: Flo) -> anyhow::Result<Self> {
        let flo_type = type_name::<T>();
        if flo.flo_type != flo_type {
            return Err(FloError::WrongContent(flo_type.to_string(), flo.flo_type).into());
        }

        let cache = T::from_rmp_bytes(flo_type, &flo.data)?;

        Ok(Self { flo, object: cache })
    }
}

impl<T: Serialize + DeserializeOwned + Clone> From<FloWrapper<T>> for Flo {
    fn from(value: FloWrapper<T>) -> Self {
        value.flo
    }
}

impl<T: Serialize + DeserializeOwned + Clone> From<&FloWrapper<T>> for Flo {
    fn from(value: &FloWrapper<T>) -> Self {
        value.flo.clone()
    }
}

#[cfg(test)]
mod test {
    use std::error::Error;

    use flarch::start_logging_filter_level;
    use flcrypto::{signer::SignerTrait, signer_ed25519::SignerEd25519};

    use super::*;

    #[test]
    fn force_id() -> anyhow::Result<()> {
        let fid = U256::rnd();
        let fw = FloWrapper::from_force_id(RealmID::rnd(), true, fid)?;
        assert_eq!(fw.flo_id(), Flo::calc_flo_id(&fid));

        Ok(())
    }

    #[tokio::test]
    async fn test_update_data() -> Result<(), Box<dyn Error>> {
        start_logging_filter_level(vec![], log::LevelFilter::Debug);

        let rid = RealmID::rnd();
        let sig = SignerEd25519::new();
        let signers = &[&sig];
        let cond = Condition::Verifier(sig.verifier());
        let mut data = "first_data".to_string();
        let flo = Flo::new_signer(rid, cond.clone(), &data, FloConfig::default(), signers)?;

        assert!(flo.verify()?);
        assert_eq!(0, flo.version());

        data = "second_data".to_string();
        let flo_new = flo.update_data_signers(cond.clone(), data.clone(), signers)?;
        assert!(flo_new.verify()?);
        assert_eq!(1, flo_new.version());
        assert_eq!(data.to_rmp_bytes(), flo_new.data);

        let sig2 = SignerEd25519::new();
        let signers2 = &[&sig2];
        let cond2 = Condition::Verifier(sig2.verifier());
        let flo2 = flo_new.update_cond_signers(cond.clone(), cond2.clone(), signers, signers2)?;
        assert!(flo2.verify()?);
        assert_eq!(1, flo2.version());
        assert_eq!(1, flo2.cond_update.len());

        let sig3 = SignerEd25519::new();
        let signers3 = &[&sig3];
        let cond3 = Condition::Verifier(sig3.verifier());
        assert!(flo2
            .update_cond_signers(cond2.clone(), cond3.clone(), signers, signers3,)
            .is_err());
        let flo_new2 = flo2.update_cond_signers(cond2, cond3, signers2, signers3)?;
        assert_eq!(2, flo_new2.cond_update.len());

        Ok(())
    }
}
