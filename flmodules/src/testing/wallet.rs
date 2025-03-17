use flarch::{broker::Broker, broker::BrokerError};
use flcrypto::{
    access::Condition,
    signer::{Signer, SignerTrait, Verifier, VerifierTrait},
    signer_ed25519::SignerEd25519,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::dht_storage::{
    broker::{DHTStorageIn, DHTStorageOut},
    core::{Cuckoo, RealmConfig},
};

use crate::flo::{
    crypto::{BadgeCond, FloBadge, FloVerifier},
    flo::{FloError, FloWrapper},
    realm::{FloRealm, RealmID},
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Testing {
    data: String,
}

pub type FloTesting = FloWrapper<Testing>;

#[derive(Error, Debug)]
pub enum WalletError {
    #[error("BrokerError({0})")]
    BrokerError(#[from] BrokerError),
    #[error("FloError({0})")]
    FloError(#[from] FloError),
}

#[derive(Default)]
pub struct Wallet {
    realm_id: RealmID,
    pub signer: Option<Signer>,
    pub verifier: Option<Verifier>,
    pub verifier_flo: Option<FloVerifier>,
    pub badge_condition: Option<Condition>,
    pub badge: Option<BadgeCond>,
    pub badge_flo: Option<FloBadge>,
    pub realm: Option<FloRealm>,
}

impl Wallet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_signer(&mut self) -> Signer {
        self.signer.get_or_insert(SignerEd25519::new()).clone()
    }

    pub fn get_verifier(&mut self) -> Verifier {
        let signer = self.get_signer();
        self.verifier.get_or_insert(signer.verifier()).clone()
    }

    pub fn get_verifier_flo(&mut self) -> FloVerifier {
        let verifier = self.get_verifier();
        let force_id = *verifier.get_id();
        self.verifier_flo
            .get_or_insert(
                FloVerifier::from_force_id(self.realm_id.clone(), verifier, force_id).unwrap(),
            )
            .clone()
    }

    pub fn get_badge_cond(&mut self) -> Condition {
        let verifier = self.get_verifier();
        self.badge_condition
            .get_or_insert(Condition::Verifier(verifier))
            .clone()
    }

    pub fn get_badge(&mut self) -> BadgeCond {
        let condition = self.get_badge_cond();
        self.badge
            .get_or_insert(BadgeCond::new(condition.to_link()))
            .clone()
    }

    pub fn get_badge_flo(&mut self) -> FloBadge {
        let badge = self.get_badge();
        let badge_rules = self.get_badge_cond();
        let signer = self.get_signer();
        self.badge_flo
            .get_or_insert(
                FloBadge::from_type(self.realm_id.clone(), badge_rules, badge, &[&signer]).unwrap(),
            )
            .clone()
    }

    pub fn create_realm(&mut self, name: &str, max_space: u64, max_flo_size: u32) -> FloRealm {
        let cond = self.get_badge_cond();
        let signer = self.get_signer();
        FloRealm::new(
            name,
            cond,
            RealmConfig {
                max_space,
                max_flo_size,
            },
            &[&signer],
        )
        .unwrap()
    }

    pub fn get_realm(&mut self) -> FloRealm {
        let new_realm = self.create_realm("root", 1_000_000, 10_000);
        self.realm.get_or_insert(new_realm).clone()
    }

    pub fn set_realm_id(&mut self, realm_id: RealmID) {
        self.verifier_flo = None;
        self.badge_flo = None;
        self.realm_id = realm_id;
        self.get_verifier_flo();
        self.get_badge_flo();
    }

    pub fn store(
        &mut self,
        dht_storage: &mut Broker<DHTStorageIn, DHTStorageOut>,
    ) -> anyhow::Result<()> {
        for flo in [self.get_verifier_flo().flo(), self.get_badge_flo().flo()] {
            dht_storage.emit_msg_in(DHTStorageIn::StoreFlo(flo.clone()))?;
        }

        Ok(())
    }
}

impl Clone for Wallet {
    fn clone(&self) -> Self {
        Self {
            realm_id: self.realm_id.clone(),
            signer: self.signer.clone(),
            verifier: self.verifier.clone(),
            verifier_flo: self.verifier_flo.clone(),
            badge_condition: self.badge_condition.clone(),
            badge: self.badge.clone(),
            badge_flo: self.badge_flo.clone(),
            realm: self.realm.clone(),
        }
    }
}

impl FloTesting {
    pub fn new(rid: RealmID, data: &str, signer: &Signer) -> Self {
        FloWrapper::from_type(
            rid,
            Condition::Pass,
            Testing {
                data: data.to_string(),
            },
            &[signer],
        )
        .unwrap()
    }

    pub fn new_cuckoo(rid: RealmID, data: &str, cuckoo: Cuckoo, signer: &Signer) -> Self {
        FloWrapper::from_type_cuckoo(
            rid,
            Condition::Pass,
            cuckoo,
            Testing {
                data: data.to_string(),
            },
            &[signer],
        )
        .unwrap()
    }
}
