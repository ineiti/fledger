use std::collections::HashMap;

use flarch::nodeids::U256;
use flcrypto::{access::Condition, signer::Signer};
use flmacro::AsU256;
use serde::{Deserialize, Serialize};

use crate::dht_storage::core::RealmConfig;

use super::flo::{Flo, FloID, FloWrapper};

pub type FloRealm = FloWrapper<Realm>;

#[derive(AsU256, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

    pub fn get_name(&self) -> String {
        self.name.clone()
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
    pub fn new(
        name: &str,
        cond: Condition,
        config: RealmConfig,
        signers: &[&Signer],
    ) -> anyhow::Result<Self> {
        Flo::new_realm(cond, &Realm::new(name.into(), config), signers)?.try_into()
    }
}

impl RealmID {
    pub fn global_id(&self, fid: FloID) -> GlobalID {
        GlobalID::new(self.clone(), fid)
    }
}

impl GlobalID {
    pub fn new(realm: RealmID, flo: FloID) -> Self {
        Self(realm, flo)
    }

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

#[cfg(test)]
mod test {
    use flarch::start_logging_filter_level;
    use flcrypto::{signer::SignerTrait, signer_ed25519::SignerEd25519, tofrombytes::ToFromBytes};

    use super::*;

    #[test]
    fn serialize() -> anyhow::Result<()> {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);

        let realm = Realm::new(
            "root".into(),
            RealmConfig {
                max_space: 1000,
                max_flo_size: 1000,
            },
        );
        let signer = SignerEd25519::new();
        let cond = Condition::Verifier(signer.verifier());
        let fr = FloRealm::from_type(RealmID::rnd(), cond.clone(), realm, &[&signer])?;
        let fr2 = fr.edit_data_signers(
            cond,
            |realm| realm.set_service("http", FloID::rnd()),
            &[&signer],
        )?;

        let realm_vec = rmp_serde::to_vec(fr2.cache())?;
        let realm_copy = rmp_serde::from_slice::<Realm>(&realm_vec)?;
        assert_eq!(fr2.cache(), &realm_copy);

        let realm_vec = fr2.cache().to_rmp_bytes();
        let realm_copy = Realm::from_rmp_bytes("name", &realm_vec)?;
        assert_eq!(fr2.cache(), &realm_copy);

        let flo2_str = serde_yaml::to_string(&fr2.flo())?;
        let fr2_flo: Flo = serde_yaml::from_str(&flo2_str)?;
        let fr3 = TryInto::<FloRealm>::try_into(fr2_flo)?;
        assert_eq!(fr2, fr3);
        Ok(())
    }
}
