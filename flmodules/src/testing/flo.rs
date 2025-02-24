use std::collections::HashMap;

use flarch::{broker::BrokerError, broker::Broker};
use flcrypto::{
    access::{Condition, Version},
    signer::{Signer, Verifier},
    signer_ed25519::SignerEd25519,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::dht_storage::{
    broker::{DHTStorageIn, DHTStorageOut},
    core::{Cuckoo, RealmConfig},
};

use crate::flo::{
    crypto::{ACEDelegation, BadgeCond, FloACE, FloBadge, FloVerifier, Rules, ACE},
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
    pub signer: Option<Box<dyn Signer>>,
    pub verifier: Option<Box<dyn Verifier>>,
    pub verifier_flo: Option<FloVerifier>,
    pub badge_condition: Option<Condition>,
    pub badge_rules: Option<Rules>,
    pub badge: Option<BadgeCond>,
    pub badge_flo: Option<FloBadge>,
    pub ace_update: Option<Condition>,
    pub ace_update_rules: Option<Rules>,
    pub ace_rules: HashMap<String, Condition>,
    pub ace_delegation: Vec<ACEDelegation>,
    pub ace: Option<ACE>,
    pub ace_flo: Option<FloACE>,
    pub realm: Option<FloRealm>,
}

impl Wallet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_signer(&mut self) -> Box<dyn Signer> {
        self.signer.get_or_insert(SignerEd25519::new_box()).clone()
    }

    pub fn get_verifier(&mut self) -> Box<dyn Verifier> {
        let signer = self.get_signer();
        self.verifier.get_or_insert(signer.verifier()).clone()
    }

    pub fn get_verifier_flo(&mut self) -> FloVerifier {
        let verifier = self.get_verifier();
        self.verifier_flo
            .get_or_insert(
                FloVerifier::from_type(self.realm_id.clone(), Rules::None, verifier).unwrap(),
            )
            .clone()
    }

    pub fn get_badge_condition(&mut self) -> Condition {
        let verifier_id = self.get_verifier().get_id();
        self.badge_condition
            .get_or_insert(Condition::Verifier(verifier_id))
            .clone()
    }

    pub fn get_badge_rules(&mut self) -> Rules {
        let condition = self.get_badge_condition();
        self.badge_rules
            .get_or_insert(Rules::Update(condition))
            .clone()
    }

    pub fn get_badge(&mut self) -> BadgeCond {
        let condition = self.get_badge_condition();
        self.badge.get_or_insert(BadgeCond::new(condition)).clone()
    }

    pub fn get_badge_flo(&mut self) -> FloBadge {
        let badge = self.get_badge();
        let badge_rules = self.get_badge_rules();
        self.badge_flo
            .get_or_insert(FloBadge::from_type(self.realm_id.clone(), badge_rules, badge).unwrap())
            .clone()
    }

    pub fn get_ace_update_rules(&mut self) -> Rules {
        let condition = self.get_badge_condition();
        self.ace_update_rules
            .get_or_insert(Rules::Update(condition))
            .clone()
    }

    pub fn get_ace_update(&mut self) -> Condition {
        let condition = self.get_badge_condition();
        self.ace_update.get_or_insert(condition).clone()
    }

    pub fn get_ace(&mut self) -> ACE {
        let update = self.get_ace_update();
        self.ace
            .get_or_insert(ACE::new(
                Some(update),
                self.ace_rules.clone(),
                self.ace_delegation.clone(),
            ))
            .clone()
    }

    pub fn get_ace_flo(&mut self) -> FloACE {
        let ace = self.get_ace();
        let update_rules = self.get_ace_update_rules();
        self.ace_flo
            .get_or_insert(FloACE::from_type(self.realm_id.clone(), update_rules, ace).unwrap())
            .clone()
    }

    pub fn get_rules(&mut self) -> Rules {
        let flo_ace = self.get_ace_flo();
        Rules::ACE(Version::Minimal(
            (*flo_ace.flo_id()).into(),
            flo_ace.version(),
        ))
    }

    pub fn get_realm(&mut self) -> FloRealm {
        let rules = self.get_badge_rules();
        self.realm
            .get_or_insert(
                FloRealm::new(
                    "root",
                    rules,
                    RealmConfig {
                        max_space: 1e6 as u64,
                        max_flo_size: 1e4 as u32,
                    },
                )
                .unwrap(),
            )
            .clone()
    }

    pub fn set_realm_id(&mut self, realm_id: RealmID) {
        self.verifier_flo = None;
        self.badge_flo = None;
        self.ace_flo = None;
        self.realm_id = realm_id;
        self.get_verifier_flo();
        self.get_badge_flo();
        self.get_ace_flo();
    }

    pub fn store(
        &mut self,
        dht_storage: &mut Broker<DHTStorageIn, DHTStorageOut>,
    ) -> Result<(), WalletError> {
        for flo in [
            self.get_verifier_flo().flo(),
            self.get_badge_flo().flo(),
            self.get_ace_flo().flo(),
        ] {
            dht_storage.emit_msg_in(DHTStorageIn::StoreFlo(flo.clone()))?;
        }

        Ok(())
    }
}

impl Clone for Wallet {
    fn clone(&self) -> Self {
        Self {
            realm_id: self.realm_id.clone(),
            signer: self.signer.as_deref().map(|s| s.clone()),
            verifier: self.verifier.clone(),
            verifier_flo: self.verifier_flo.clone(),
            badge_condition: self.badge_condition.clone(),
            badge_rules: self.badge_rules.clone(),
            badge: self.badge.clone(),
            badge_flo: self.badge_flo.clone(),
            ace_update: self.ace_update.clone(),
            ace_update_rules: self.ace_update_rules.clone(),
            ace_rules: self.ace_rules.clone(),
            ace_delegation: self.ace_delegation.clone(),
            ace: self.ace.clone(),
            ace_flo: self.ace_flo.clone(),
            realm: self.realm.clone(),
        }
    }
}

impl FloTesting {
    pub fn new(rid: RealmID, data: &str) -> Self {
        FloWrapper::from_type(
            rid,
            Rules::Update(Condition::Pass),
            Testing {
                data: data.to_string(),
            },
        )
        .unwrap()
    }

    pub fn new_cuckoo(rid: RealmID, data: &str, cuckoo: Cuckoo) -> Self {
        FloWrapper::from_type_cuckoo(
            rid,
            Rules::Update(Condition::Pass),
            cuckoo,
            Testing {
                data: data.to_string(),
            },
        )
        .unwrap()
    }
}
