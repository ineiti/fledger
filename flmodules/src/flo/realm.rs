use std::collections::HashMap;

use flarch::nodeids::U256;
use flmacro::AsU256;
use serde::{Deserialize, Serialize};

use crate::dht_storage::core::RealmConfig;

use super::{
    crypto::Rules,
    flo::{Flo, FloError, FloID, FloWrapper},
};

pub type FloRealm = FloWrapper<Realm>;

#[derive(AsU256, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RealmID(U256);

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Realm {
    /// The name cannot be modified and must stay the same.
    name: String,
    /// Configuration options for the DHT hosting this realm
    config: RealmConfig,
    /// Pointers to root Flos for different services.
    services: HashMap<String, FloID>,
}

/// A GlobalID holds the RealmID and the specific FloID.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct GlobalID(RealmID, FloID);

impl Realm {
    pub fn new(name: String, config: RealmConfig) -> Self {
        Self {
            name,
            config,
            services: HashMap::new(),
        }
    }

    pub fn get_config(&self) -> RealmConfig {
        self.config.clone()
    }

    pub fn set_service(&mut self, name: &str, id: FloID) {
        self.services.insert(name.into(), id);
    }

    pub fn get_services(&self) -> &HashMap<String, FloID> {
        &self.services
    }
}

impl FloRealm {
    pub fn new(name: &str, rules: Rules, config: RealmConfig) -> Result<Self, FloError> {
        Flo::new_realm(rules, &Realm::new(name.into(), config))?.try_into()
    }
}

impl GlobalID {
    pub fn new(realm: RealmID, flo: FloID) -> Self {
        Self(realm, flo)
    }

    // pub fn node_id(&self) -> NodeID {
    //     NodeID::hash_domain_parts(
    //         "re.fledg.flmodules.flo.global_id",
    //         &[self.0.as_ref(), self.1.as_ref()],
    //     )
    // }

    pub fn realm_id(&self) -> &RealmID {
        &self.0
    }

    pub fn flo_id(&self) -> &FloID {
        &self.1
    }
}

impl From<&RealmID> for GlobalID {
    fn from(value: &RealmID) -> Self {
        GlobalID::new(value.clone(), (*value.clone()).into())
    }
}

impl Default for RealmID {
    fn default() -> Self {
        RealmID::zero()
    }
}
