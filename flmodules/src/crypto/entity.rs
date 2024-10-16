use std::{collections::HashMap, io::Write};

use bytes::Bytes;
use flarch::nodeids::U256;
use futures::executor::EnterError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::signer::{Signature, Signer, SignerError, Verifier};

/**
 * An entity has 1 or more verifiers with the following methods:
 * - new(ver_1, ver_2, ...) -> Entity // This is Entity_0
 * - get_id(Entity) -> EntityID // the EntityID stays constant over all entity evolutions
 * - propose_signature(Entity, msg) -> EntitySigCollect
 * - sign(EntitySigCollect, id_n, secret_n) -> EntitySigCollect
 * - finalize(EntitySigCollect) -> EntitySig?
 * - verify(Entity, EntitySig, msg) -> bool
 * - propose_evolve(Entity, id_1, id_2, ...) -> EntitySigCollect
 * - evolve(Entity_n, EntitySig, id_1, id_2, ...) -> Entity_n+1
 * - verify_entity(Entity, EntityID)
 */

#[derive(Debug, Error)]
pub enum EntityError {
    #[error("Verification error: not enough signatures")]
    VerificationSignatureMissing,
    #[error("Verification error: wrong signatures present")]
    VerificationWrongSignatures,
    #[error("Cannot evolve with this signature")]
    EvolveWrongSignature,
    #[error("The EntityID doesn't match this entity")]
    WrongEntityID,
    #[error("Signature error: identity not in Collection")]
    SignatureWrongIdentity,
    #[error("Signature error: wrong type of signature for this entity")]
    SignatureWrongEntity,
    #[error("Signature error")]
    SignatureError(#[from] SignerError),
    #[error("Not enough signatures present")]
    ThresholdNotReached,
    #[error("Cannot have zero threshold")]
    ZeroThreshold,
    #[error("Not enough verifiers for this threshold")]
    ThresholdAboveVerifiers,
    #[error("Need at least one verifier")]
    ZeroVerifiers,
    #[error("Not the same evolution state")]
    EvolutionState,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum EntityVerifierV1 {
    Verifier(Verifier),
    Entity(Entity),
}

#[derive(Serialize, Deserialize, Clone)]
pub enum Entity {
    V1(EntityV1),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EntityV1 {
    initial: EntityHistoryV1,
    updates: Vec<EntityHistoryV1>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EntityHistoryV1 {
    entities: Vec<EntityVerifierV1>,
    threshold: usize,
    sigs: Option<EntitySignature>,
}

pub type EntityID = U256;

impl Entity {
    pub fn new(entities: &[EntityVerifierV1], threshold: usize) -> Result<Self, EntityError> {
        if threshold == 0 {
            return Err(EntityError::ZeroThreshold);
        }
        if threshold > entities.len() {
            return Err(EntityError::ThresholdAboveVerifiers);
        }
        if entities.len() == 0 {
            return Err(EntityError::ZeroVerifiers);
        }

        Ok(Entity::V1(EntityV1 {
            initial: EntityHistoryV1 {
                entities: entities.into(),
                threshold,
                sigs: None,
            },
            updates: vec![],
        }))
    }

    pub fn get_id(&self) -> EntityID {
        let mut hasher = Sha256::new();
        for entity in &self.entity().initial.entities {
            hasher.update(entity.get_id());
        }
        hasher.finalize().into()
    }

    pub fn get_entities(&self) -> Vec<EntityVerifierV1> {
        self.latest().get_entities()
    }

    pub fn propose_signature(&self, msg: &Bytes) -> EntitySigCollect {
        EntitySigCollect::new(
            self.entity_msg(self.version(), msg),
            self.latest()
                .entities
                .iter()
                .map(|ent| ent.get_id())
                .collect(),
            self.latest().threshold,
        )
    }

    pub fn verify(&self, sig: &EntitySignature, msg: &Bytes) -> Result<(), EntityError> {
        self.latest()
            .verify(sig, &self.entity_msg(self.version(), msg))
    }

    fn verify_raw(&self, sig: &EntitySignature, msg: &Bytes) -> Result<(), EntityError> {
        self.latest()
            .verify(sig, msg)
    }

    pub fn propose_evolve(
        &self,
        entities_new: &[EntityVerifierV1],
        threshold: usize,
    ) -> Result<EntitySigCollect, EntityError> {
        if threshold == 0 {
            return Err(EntityError::ZeroThreshold);
        }
        if threshold > entities_new.len() {
            return Err(EntityError::ThresholdAboveVerifiers);
        }
        if entities_new.len() == 0 {
            return Err(EntityError::ZeroVerifiers);
        }

        Ok(self.propose_signature(&self.latest().propose_msg(entities_new, threshold)))
    }

    /// When signing an EntitySigCollect for an evolution, this method allows to check whether the
    /// proposed evolution is actually what the signer supposes it should be.
    pub fn sign_evolve(&self, signer: &Signer, sig_coll: &mut EntitySigCollect, entities_new: &[EntityVerifierV1], threshold: usize) -> Result<(), EntityError>{
        let msg = self.entity_msg(self.version(), &self.latest().propose_msg(entities_new, threshold));
        if msg != sig_coll.msg {
            return Err(EntityError::EvolutionState);
        }
        sig_coll.sign(signer)
    }

    pub fn evolve(
        &mut self,
        sig: EntitySignature,
        entities_new: &[EntityVerifierV1],
        threshold: usize,
    ) -> Result<(), EntityError> {
        self.verify_raw(
            &sig,
            &self.propose_evolve(entities_new, threshold)?.get_msg(),
        )?;
        let s = self.entity_mut();
        if let Some(last) = s.updates.last_mut() {
            last.sigs = Some(sig);
        } else {
            s.initial.sigs = Some(sig);
        }
        s.updates.push(EntityHistoryV1 {
            entities: entities_new.into(),
            threshold: s.initial.threshold,
            sigs: None,
        });
        Ok(())
    }

    pub fn verify_entity(&self, id: EntityID) -> Result<(), EntityError> {
        if id != self.get_id() {
            return Err(EntityError::WrongEntityID);
        }
        let mut version = 0;
        for pair in [
            vec![&self.entity().initial],
            self.entity().updates.iter().collect(),
        ]
        .concat()
        .windows(2)
        {
            let msg = self.entity_msg(
                version,
                &pair[0].propose_msg(&pair[1].entities, pair[1].threshold),
            );
            match &pair[0].sigs {
                Some(sigs) => pair[0].verify(sigs, &msg)?,
                None => return Err(EntityError::EvolveWrongSignature),
            }
            version += 1;
        }
        Ok(())
    }

    pub fn version(&self) -> usize {
        self.entity().updates.len()
    }

    fn entity_msg(&self, version: usize, msg: &Bytes) -> Bytes {
        let mut hasher = Sha256::new();
        hasher.update(version.to_le_bytes());
        hasher.update(&self.get_id().to_bytes());
        hasher.update(&msg);
        Bytes::from(hasher.finalize().to_vec())
    }

    fn latest(&self) -> EntityHistoryV1 {
        self.entity()
            .updates
            .last()
            .unwrap_or(&self.entity().initial)
            .clone()
    }

    fn entity_mut(&mut self) -> &mut EntityV1 {
        match self {
            Entity::V1(entity_v1) => entity_v1,
        }
    }

    fn entity(&self) -> &EntityV1 {
        match self {
            Entity::V1(entity_v1) => entity_v1,
        }
    }
}

impl EntityHistoryV1 {
    pub fn get_entities(&self) -> Vec<EntityVerifierV1> {
        self.entities.iter().cloned().collect()
    }

    fn verify(&self, sigs: &EntitySignature, msg: &Bytes) -> Result<(), EntityError> {
        match sigs {
            EntitySignature::Single(sig) => {
                if self.threshold > 1 {
                    return Err(EntityError::ThresholdNotReached);
                }
                if self.entities.len() > 1 {
                    return Err(EntityError::SignatureWrongEntity);
                }

                if let Some(EntityVerifierV1::Verifier(ver)) = self.entities.get(0) {
                    Ok(ver.verify(msg, sig)?)
                } else {
                    Err(EntityError::SignatureWrongEntity)
                }
            }
            EntitySignature::Entity(esigs) => todo!(),
        }
    }

    fn propose_msg(&self, entities_new: &[EntityVerifierV1], threshold: usize) -> Bytes {
        let mut hasher = Sha256::new();
        hasher.update(threshold.to_le_bytes());
        for entity in entities_new {
            hasher.update(&entity.get_id());
        }

        hasher.finalize().to_vec().into()
    }
}

pub struct EntitySigCollect {
    msg: Bytes,
    signed: HashMap<EntityID, Option<EntitySignature>>,
    threshold: usize,
}

impl EntitySigCollect {
    pub fn new(msg: Bytes, ids: Vec<EntityID>, threshold: usize) -> Self {
        Self {
            msg,
            signed: ids.into_iter().map(|id| (id, None)).collect(),
            threshold,
        }
    }

    pub fn sign(&mut self, sign: &Signer) -> Result<(), EntityError> {
        match self.signed.get_mut(&sign.get_id()) {
            Some(sig) => *sig = Some(EntitySignature::Single(sign.sign(&self.msg)?)),
            None => return Err(EntityError::WrongEntityID),
        }
        Ok(())
    }

    pub fn add_sig(&mut self, id: EntityID, esig: EntitySignature) -> Result<(), EntityError> {
        match self.signed.get_mut(&id) {
            Some(sig) => *sig = Some(esig),
            None => return Err(EntityError::WrongEntityID),
        }
        Ok(())
    }

    pub fn get_msg(&self) -> Bytes {
        self.msg.clone()
    }

    pub fn finalize(&self) -> Result<EntitySignature, EntityError> {
        if self.signed.values().filter(|sig| sig.is_some()).count() < self.threshold {
            return Err(EntityError::ThresholdNotReached);
        }
        let kvs: Vec<(&U256, &Option<EntitySignature>)> = self.signed.iter().collect();
        if kvs.len() == 1 {
            if let Some((_, Some(EntitySignature::Single(sig)))) = kvs.first() {
                return Ok(EntitySignature::Single(sig.clone()));
            }
        }
        let esigs = kvs
            .iter()
            .filter_map(|(&id, op)| op.as_ref().map(|es| (id, es.clone())))
            .collect();
        Ok(EntitySignature::Entity(esigs))
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub enum EntitySignature {
    Single(Signature),
    Entity(HashMap<EntityID, EntitySignature>),
}

impl From<Signer> for EntityVerifierV1 {
    fn from(value: Signer) -> Self {
        Self::Verifier(value.verifier())
    }
}

impl From<Verifier> for EntityVerifierV1 {
    fn from(value: Verifier) -> Self {
        Self::Verifier(value)
    }
}

impl From<Entity> for EntityVerifierV1 {
    fn from(value: Entity) -> Self {
        Self::Entity(value)
    }
}

impl EntityVerifierV1 {
    pub fn get_id(&self) -> EntityID {
        match self {
            EntityVerifierV1::Verifier(verifier) => verifier.get_id(),
            EntityVerifierV1::Entity(entity) => entity.get_id(),
        }
    }
}

#[cfg(test)]
mod test {
    use std::error::Error;

    use crate::crypto::signer::SignatureType;

    use super::*;

    type TR = Result<(), Box<dyn Error>>;

    #[test]
    fn test_sig_single() -> TR {
        let sig = Signer::new(SignatureType::Ed25519);
        assert!(Entity::new(&[], 1).is_err());
        assert!(Entity::new(&[sig.verifier().into()], 0).is_err());
        assert!(Entity::new(&[sig.verifier().into()], 2).is_err());
        let ent = Entity::new(&[sig.verifier().into()], 1)?;
        let msg = Bytes::from("signature message");
        let mut sig_ent = ent.propose_signature(&msg);
        assert!(sig_ent.finalize().is_err());
        sig_ent.sign(&sig)?;
        let sig_ent_final = sig_ent.finalize()?;
        assert!(ent.verify(&sig_ent_final, &msg).is_ok());
        assert!(ent
            .verify(&sig_ent_final, &Bytes::from("message2"))
            .is_err());

        Ok(())
    }

    #[test]
    fn test_sig_threshold_1() -> TR {
        let sig1 = Signer::new(SignatureType::Ed25519);
        let sig2 = Signer::new(SignatureType::Ed25519);
        let ent = Entity::new(&[sig1.verifier().into(), sig2.verifier().into()], 1)?;
        let msg = Bytes::from("signature message");
        let mut sig_ent = ent.propose_signature(&msg);
        sig_ent.sign(&sig1)?;
        let sig_ent_final = sig_ent.finalize()?;
        assert!(ent.verify(&sig_ent_final, &msg).is_ok());

        let mut sig_ent = ent.propose_signature(&msg);
        sig_ent.sign(&sig2)?;
        let sig_ent_final = sig_ent.finalize()?;
        assert!(ent.verify(&sig_ent_final, &msg).is_ok());

        Ok(())
    }

    #[test]
    fn test_evolve_single() -> TR {
        let sig = Signer::new(SignatureType::Ed25519);
        let mut ent = Entity::new(&[sig.verifier().into()], 1)?;
        let id = ent.get_id();
        let sig2 = Signer::new(SignatureType::Ed25519);

        let mut sig_evolve_propose = ent.propose_evolve(&[sig2.verifier().into()], 1)?;
        assert!(sig_evolve_propose.finalize().is_err());
        assert!(sig_evolve_propose.sign(&sig2).is_err());
        assert!(sig_evolve_propose.sign(&sig).is_ok());
        assert!(sig_evolve_propose.sign(&sig).is_ok());

        let sig_evolve = sig_evolve_propose.finalize()?;
        assert!(ent
            .evolve(sig_evolve.clone(), &[sig.verifier().into()], 1)
            .is_err());
        assert!(ent
            .evolve(sig_evolve.clone(), &[sig2.verifier().into()], 2)
            .is_err());
        ent.evolve(sig_evolve, &[sig2.verifier().into()], 1)?;
        assert_eq!(1, ent.version());
        assert_eq!(id, ent.get_id());

        Ok(())
    }
}
