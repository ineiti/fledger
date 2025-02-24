use std::collections::HashMap;

use flarch::data_storage::DataStorage;
use flcrypto::{access::Condition, signer::Signer, signer_ed25519::SignerEd25519};
use serde::{Deserialize, Serialize};

use super::{
    crypto::{FloACE, Rules, ACE},
    realm::RealmID,
};

#[derive(Serialize, Deserialize, Default)]
pub struct CryptoStorage {
    #[serde(skip)]
    ds: Option<Box<dyn DataStorage + Send>>,
    realm_id: RealmID,
    pub signers: Vec<Signer>,
    pub aces: Vec<FloACE>,
}

const STORAGE_NAME: &str = "CryptoStorage";

impl CryptoStorage {
    pub fn new(ds: Box<dyn DataStorage + Send>) -> Self {
        Self {
            ds: Some(ds.clone()),
            ..serde_yaml::from_str(&ds.get(STORAGE_NAME).unwrap_or("".to_string()))
                .unwrap_or(CryptoStorage::default())
        }
    }

    pub fn get_signer(&mut self) -> Signer {
        if let Some(signer) = self.signers.first() {
            return (*signer).clone();
        }
        self.add_signer()
    }

    pub fn add_signer(&mut self) -> Signer {
        self.signers.push(SignerEd25519::new());
        self.store();
        self.get_signer()
    }

    pub fn get_ace(&mut self) -> FloACE {
        if let Some(fa) = self.aces.first() {
            return fa.clone();
        }
        self.add_ace()
    }

    pub fn add_ace(&mut self) -> FloACE {
        let signer = self.get_signer();
        let cond = Condition::Verifier(signer.verifier().get_id());
        let ace = ACE::new(Some(cond.clone()), HashMap::new(), vec![]);
        self.aces.push(
            FloACE::from_type(self.realm_id.clone(), Rules::Update(cond), ace)
                .expect("Creating ACE"),
        );
        self.get_ace()
    }

    pub fn store(&mut self) {
        let ser = serde_yaml::to_string(&self).unwrap_or("".to_string());
        self.ds.as_mut().map(|ds| ds.set(STORAGE_NAME, &ser));
    }
}
