use bytes::Bytes;
use flarch::{nodeids::U256, tasks::now};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::crypto::{access::TesseraID, signer::Signature};

#[derive(Debug, Error)]
pub enum FloError {
    #[error("Couldn't deserialize {0}: '{1:?}'")]
    Deserialization(String, String),
    #[error("Couldn't serialize: {0}")]
    Serialization(String),
    #[error("Expected content to be '{0:?}', but was '{1:?}")]
    WrongContent(Content, Content),
    #[error("While converting flo to {0:?}: {1}")]
    Conversion(Content, String),
    #[error("This update has no Data")]
    UpdateNoData,
    #[error("This update has no ACE")]
    UpdateNoACE,
    #[error("No Updates")]
    UpdatesMissing,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq)]
pub struct Flo {
    pub id: U256,
    pub content: Content,
    pub updates: Vec<Update>,
}

impl Flo {
    pub fn new_now(content: Content, data: String, ace: ACE) -> Self {
        Self::new(now() as u64, content, data, ace)
    }

    pub fn new(now: u64, content: Content, data: String, ace: ACE) -> Self {
        let updates = vec![
            Update {
                time: now,
                change: Change::ACE(ace),
                proof: Proof::Initial,
            },
            Update {
                time: now,
                change: Change::Data(data),
                proof: Proof::Initial,
            },
        ];
        let mut hasher = Sha256::new();
        hasher.update(now.to_le_bytes());
        hasher.update(content.to_bytes());
        for update in &updates {
            hasher.update(update.to_bytes());
        }
        Self {
            id: hasher.finalize().into(),
            content,
            updates,
        }
    }

    pub fn data(&self) -> String {
        self.updates
            .iter()
            .rev()
            .find(|u| matches!(u.change, Change::Data(_)))
            .unwrap()
            .change
            .data()
            .unwrap()
    }

    pub fn ace(&self) -> ACE {
        self.updates
            .iter()
            .rev()
            .find(|u| matches!(u.change, Change::ACE(_)))
            .unwrap()
            .change
            .ace()
            .unwrap()
    }

    pub fn is_valid(&self) -> Result<(), FloError> {
        if self.updates.len() < 2 {
            return Err(FloError::UpdatesMissing);
        }

        if self
            .updates
            .iter()
            .rev()
            .find(|u| matches!(u.change, Change::ACE(_)))
            .is_none()
        {
            return Err(FloError::Deserialization(
                "Flo".into(),
                "Missing ACE".into(),
            ));
        }

        if self
            .updates
            .iter()
            .rev()
            .find(|u| matches!(u.change, Change::Data(_)))
            .is_none()
        {
            return Err(FloError::Deserialization(
                "Flo".into(),
                "Missing Data".into(),
            ));
        }
        Ok(())
    }

    pub fn version(&self) -> usize {
        self.updates.len()
    }
}

impl TryFrom<String> for Flo {
    type Error = FloError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let flo = match serde_yaml::from_str::<Self>(&value) {
            Ok(flo) => flo,
            Err(e) => return Err(FloError::Deserialization("Flo".into(), e.to_string())),
        };
        flo.is_valid()?;
        Ok(flo)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Default, Clone)]
pub enum Content {
    #[default]
    Domain,
    DHTConfig,
    FloEntity,
    LedgerConfig,
    Mana,
    Blob,
}

impl Content {
    pub fn to_bytes(&self) -> Bytes {
        format!("{:?}", self).into()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Update {
    pub time: u64,
    pub change: Change,
    pub proof: Proof,
}

impl Update {
    pub fn to_bytes(&self) -> Bytes {
        format!("{}::{:?}::{:?}", self.time, self.change, self.proof).into()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum Change {
    Data(String),
    ACE(ACE),
}

impl Change {
    pub fn data(&self) -> Result<String, FloError> {
        match self {
            Change::Data(str) => Ok(str.clone()),
            Change::ACE(_) => Err(FloError::UpdateNoData),
        }
    }

    pub fn ace(&self) -> Result<ACE, FloError> {
        match self {
            Change::ACE(ace) => Ok(ace.clone()),
            Change::Data(_) => Err(FloError::UpdateNoACE),
        }
    }

    pub fn size(&self) -> usize {
        match self {
            Change::Data(str) => str.len(),
            Change::ACE(ace) => ace.rules.iter().map(|r| r.action.size()).sum(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum Proof {
    Initial,
    Signature(Signature),
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct ACE {
    pub rules: Vec<Rule>,
}

impl ACE {
    pub fn new(rules: Vec<Rule>) -> Self {
        Self { rules }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Rule {
    pub action: Action,
    pub condition: Condition,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum Action {
    UpdateACE,
    UpdateData,
    Flo(String),
}

impl Action {
    pub fn size(&self) -> usize {
        match self {
            Action::UpdateACE => 0,
            Action::UpdateData => 0,
            Action::Flo(str) => str.len(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum Condition {
    Signature(TesseraID),
}
