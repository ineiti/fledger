use std::{any::type_name, collections::HashMap};

use bytes::Bytes;
use flarch::nodeids::U256;
use flmacro::AsU256;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_with::{hex::Hex, serde_as};
use thiserror::Error;

use flcrypto::{
    access::{Badge, BadgeSignature, Condition},
    signer::{KeyPairID, Signature, Signer, SignerError, Verifier},
    tofrombytes::{TFBError, ToFromBytes},
};

use crate::dht_storage::core::{Cuckoo, FloConfig};

use super::{
    crypto::Rules,
    realm::{GlobalID, Realm, RealmID},
};

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
}

#[derive(AsU256, Deserialize, Serialize, Clone, PartialEq, Eq, Hash)]
pub struct FloID(U256);

/// Flo defines the following actions:
/// - ican.re.fledg.flmodules.flo.flo
///   - .update_data - update the data
///   - .update_ace - update the Version<AceID>
#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Flo {
    // In which realm this Flo resides.
    realm: RealmID,
    // What type this Flo is - should be a generic mime-type
    flo_type: String,
    // Does it allow storing of nearby Cuckoos?
    flo_config: FloConfig,
    #[serde_as(as = "Hex")]
    // The current data of the Flo
    data: Bytes,
    // All history to prove that the current Flo is legit.
    history: History,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct History {
    // The first version of this Flo
    genesis: HistoryStep,
    // Every new version needs to include the hashes and the new Rules for
    // the update, as well as the Verifiers used for the signatures.
    evolve: Vec<HistoryStep>,
}

/// TODO: work out what the best strategy is for the signature and the
/// ID of the flo:
/// 1. ID = H(content, genesis.data_hash, genesis.ace_version), Sign(last_ID, new_ID)
/// - seems easiest and most straightforward
/// 2. Hash_n = H(content, version_n.data_hash, version_n.ace_version), Sign(Hash_n-1, Hash_n),
/// ID = H(Hash_n, Signature_n)
/// - seems more complicated
/// - allows for adding a nonce to the Flo
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HistoryStep {
    // Hash of the data of this step - needed to be able to re-create the message which
    // is signed to prove the update.
    data_hash: U256,
    // The new rules of this step. Might be the same as the previous one.
    rules: Rules,
    // It is OK to have the verifiers here, because their IDs are in the hash of the FloID.
    // So an attacker cannot change the verifiers to create a wrong signature.
    verifiers: Vec<Box<dyn Verifier>>,
    // All signatures to prove evolution to the next version.
    signatures: HashMap<KeyPairID, Signature>,
}

/// A convenience method to handle a type T together with its
/// Flo-representation of all the history and ACE.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FloWrapper<T> {
    flo: Flo,
    object: T,
}

pub struct UpdateSign<T: Serialize + DeserializeOwned + Clone> {
    pub update: T,
    pub rules: Rules,
    pub signatures: BadgeSignature,
}

impl Flo {
    pub fn new<T: Serialize + DeserializeOwned>(
        realm: RealmID,
        rules: Rules,
        data: &T,
        flo_config: FloConfig,
    ) -> Result<Self, FloError> {
        let data: Bytes = rmp_serde::to_vec(data)?.into();
        let flo_type = type_name::<T>().into();
        Ok(Self {
            realm,
            flo_type,
            flo_config,
            history: History {
                genesis: HistoryStep {
                    data_hash: U256::hash_data(&data),
                    rules,
                    verifiers: vec![],
                    signatures: HashMap::new(),
                },
                evolve: vec![],
            },
            data,
        })
    }

    pub fn new_realm(rules: Rules, realm: &Realm) -> Result<Self, FloError> {
        let mut f = Self::new(RealmID::zero(), rules, realm, FloConfig::default())?;
        f.realm = (*f.flo_id()).into();
        Ok(f)
    }

    pub fn data(&self) -> Bytes {
        self.data.clone()
    }

    pub fn latest_step(&self) -> &HistoryStep {
        if let Some(step) = self.history.evolve.last() {
            return step;
        }
        &self.history.genesis
    }

    pub fn rules(&self) -> &Rules {
        &self.latest_step().rules
    }

    pub fn version(&self) -> u32 {
        self.history.evolve.len() as u32
    }

    pub fn flo_id(&self) -> FloID {
        FloID::hash_domain_parts(
            "ican.re.fledg.flmodules.flo.Flo",
            &[
                &self.flo_type.to_bytes(),
                self.history.genesis.data_hash.as_ref(),
                &self.history.genesis.rules.to_bytes(),
            ],
        )
    }

    pub fn global_id(&self) -> GlobalID {
        GlobalID::new(self.realm.clone(), self.flo_id())
    }

    pub fn realm_id(&self) -> RealmID {
        self.realm.clone()
    }

    pub fn update<T: Serialize + DeserializeOwned + Clone>(
        &self,
        update: &mut UpdateSign<T>,
    ) -> Result<Flo, SignerError> {
        let data = update.update.to_bytes();
        let signatures = update.signatures.finalize()?;
        let mut next_version = self.clone();
        next_version.history.evolve.push(HistoryStep {
            data_hash: U256::hash_data(&data),
            rules: update.rules.clone(),
            verifiers: vec![],
            signatures,
        });
        next_version.data = data;
        Ok(next_version)
    }

    pub fn flo_type(&self) -> String {
        self.flo_type.clone()
    }

    pub fn flo_config(&self) -> &FloConfig {
        &self.flo_config
    }
}

impl<T: Serialize + DeserializeOwned + Clone> FloWrapper<T> {
    pub fn from_type(realm: RealmID, rules: Rules, object: T) -> Result<Self, FloError> {
        Self::from_type_config(realm, rules, FloConfig::default(), object)
    }

    pub fn from_type_cuckoo(
        realm: RealmID,
        rules: Rules,
        cuckoo: Cuckoo,
        object: T,
    ) -> Result<Self, FloError> {
        Self::from_type_config(realm, rules, FloConfig { cuckoo }, object)
    }

    pub fn from_type_config(
        realm: RealmID,
        rules: Rules,
        config: FloConfig,
        object: T,
    ) -> Result<Self, FloError> {
        let flo = Flo::new(realm, rules, &object, config)?;
        Ok(Self { flo, object })
    }

    pub fn cache(&self) -> &T {
        &self.object
    }

    pub fn flo(&self) -> &Flo {
        &self.flo
    }

    pub fn rules(&self) -> &Rules {
        self.flo.rules()
    }

    pub fn data(&self) -> &Bytes {
        &self.flo.data
    }

    pub fn version(&self) -> u32 {
        self.flo.history.evolve.len() as u32
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
        let new_hash = U256::hash_data(&update.to_bytes()).to_bytes();
        U256::hash_domain_parts("FloUpdate", &[&self.hash().to_bytes(), &new_hash])
    }

    pub fn start_sign(
        &self,
        condition: Condition,
        rules: Rules,
        badges: Vec<Box<dyn Badge>>,
        update: T,
    ) -> Result<UpdateSign<T>, SignerError> {
        let msg_orig = self.hash_update(&update).bytes();
        let signatures =
            BadgeSignature::from_cond(condition.clone(), msg_orig, &move |vi| -> Option<
                Box<dyn Badge>,
            > {
                badges
                    .iter()
                    .find(|b| b.badge_id() == vi.get_id() && vi.accepts(b.badge_version()))
                    .cloned()
            })?;
        Ok(UpdateSign {
            update,
            rules,
            signatures,
        })
    }

    pub fn edit_sign_update(
        &mut self,
        edit: impl FnOnce(&mut T),
        cond: Condition,
        rules: Rules,
        badges: Vec<Box<dyn Badge>>,
        signers: &[&dyn Signer],
    ) -> Result<u32, SignerError> {
        let update = self.edit(edit);
        let mut u_s = self.start_sign(cond, rules, badges, update)?;
        for &signer in signers {
            u_s.sign(signer)?;
        }
        self.apply_update(u_s)
    }

    pub fn apply_update(&mut self, mut update: UpdateSign<T>) -> Result<u32, SignerError> {
        self.flo = self.flo.update(&mut update)?;
        self.object = update.update;
        Ok(self.version())
    }

    pub fn hash(&self) -> U256 {
        U256::hash_domain_parts("FloWrapper", &[&self.flo.to_bytes()])
    }
}

impl<T: Serialize + DeserializeOwned + Clone> UpdateSign<T> {
    pub fn new(update: T, rules: Rules, signatures: BadgeSignature) -> Self {
        Self {
            update,
            rules,
            signatures,
        }
    }

    pub fn sign(&mut self, signer: &dyn Signer) -> Result<(), SignerError> {
        self.signatures.sign(signer)
    }
}

impl<T: Serialize + DeserializeOwned> TryFrom<Flo> for FloWrapper<T> {
    type Error = FloError;

    fn try_from(flo: Flo) -> Result<Self, Self::Error> {
        let flo_type = type_name::<T>();
        if flo.flo_type != flo_type {
            return Err(FloError::WrongContent(flo_type.to_string(), flo.flo_type));
        }

        let cache = T::from_bytes(flo_type, &flo.data)?;

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
