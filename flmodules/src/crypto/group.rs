use bytes::Bytes;
use flarch::nodeids::U256;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::{entity::Entity, signer::Signer};

/**
 * A group has 2 or more identities with the following methods:
 * - new(id_1, id_2, ...) -> Group // This is Group_0
 * - get_id(Group) -> GroupID // the GroupID stays constant over all group evolutions
 * - propose_signature(Group, msg) -> GroupSigCollect
 * - sign(GroupSigCollect, id_n, secret_n) -> GroupSigCollect
 * - finalize(GroupSigCollect) -> GroupSig?
 * - verify(Group, GroupSig, msg) -> bool
 * - propose_evolve(Group, id_1, id_2, ...) -> GroupSigCollect
 * - evolve(Group_n, GroupSig, id_1, id_2, ...) -> Group_n+1
 * - verify_group(Group, GroupID)
 */

#[derive(Debug, Error)]
pub enum GroupError {
    #[error("Verification error: not enough signatures")]
    VerificationSignatureMissing,
    #[error("Verification error: wrong signatures present")]
    VerificationWrongSignatures,
    #[error("Cannot evolve with this signature")]
    EvolveWrongSignature,
    #[error("The GroupID doesn't match this group")]
    WrongGroupID,
    #[error("Signature error: identity not in Collection")]
    SignatureWrongIdentity,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GroupVerifier {
    initial: GroupHistory,
    updates: Vec<GroupHistory>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GroupHistory {
    entities: Vec<Entity>,
    threshold: usize,
    sigs: Option<GroupSig>,
}

pub type GroupID = U256;

impl GroupVerifier {
    pub fn new(entities: Vec<Entity>, threshold: usize) -> Self {
        Self {
            initial: GroupHistory {
                entities,
                threshold,
                sigs: None,
            },
            updates: vec![],
        }
    }

    pub fn get_id(&self) -> GroupID {
        let mut hasher = Sha256::new();
        for entity in &self.initial.entities {
            hasher.update(entity.get_id());
        }
        hasher.finalize().into()
    }

    pub fn get_entities(&self) -> Vec<Entity> {
        self.latest().get_entities()
    }

    pub fn propose_signature(&self, msg: Bytes) -> GroupSigCollect {
        todo!()
    }

    pub fn verify(&self, sig: &GroupSig, msg: &Bytes) -> Result<(), GroupError> {
        self.latest().verify(sig, msg)
    }

    pub fn propose_evolve(&self, entities_new: &[Entity], threshold: usize) -> GroupSigCollect {
        self.latest().propose_evolve(entities_new, threshold)
    }

    pub fn evolve(
        &mut self,
        sig: GroupSig,
        entities_new: Vec<Entity>,
        threshold: usize,
    ) -> Result<(), GroupError> {
        self.verify(
            &sig,
            &self.propose_evolve(&entities_new, threshold).get_msg(),
        )?;
        if self.updates.len() == 0 {
            self.initial.sigs = Some(sig);
        } else {
            self.latest().sigs = Some(sig);
        }
        self.updates.push(GroupHistory {
            entities: entities_new,
            threshold: self.initial.threshold,
            sigs: None,
        });
        Ok(())
    }

    pub fn verify_group(&self, id: GroupID) -> Result<(), GroupError> {
        if id != self.get_id() {
            return Err(GroupError::WrongGroupID);
        }
        for pair in [vec![&self.initial], self.updates.iter().collect()]
            .concat()
            .windows(2)
        {
            let msg = pair[0]
                .propose_evolve(&pair[1].entities, pair[1].threshold)
                .get_msg();
            match &pair[0].sigs {
                Some(sigs) => pair[0].verify(sigs, &msg)?,
                None => return Err(GroupError::EvolveWrongSignature),
            }
        }
        Ok(())
    }

    pub fn latest(&self) -> GroupHistory {
        self.updates.last().unwrap_or(&self.initial).clone()
    }
}

impl GroupHistory {
    pub fn get_entities(&self) -> Vec<Entity> {
        self.entities.iter().cloned().collect()
    }

    pub fn verify(&self, sig: &GroupSig, msg: &Bytes) -> Result<(), GroupError> {
        todo!()
    }

    pub fn propose_evolve(&self, entities_new: &[Entity], threshold: usize) -> GroupSigCollect {
        todo!()
    }
}

pub struct GroupSigCollect {}

impl GroupSigCollect {
    pub fn sign(&self, id: Entity, sign: Signer) -> Result<Self, GroupError> {
        todo!()
    }

    pub fn get_msg(&self) -> Bytes {
        todo!()
    }

    pub fn finalize(&self) -> Result<GroupSig, GroupError> {
        todo!()
    }
}

pub type GroupSig = Bytes;
