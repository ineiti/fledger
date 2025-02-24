use std::collections::HashMap;

use flmacro::VersionedSerde;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use flarch::{
    nodeids::{NodeID, U256},
    tasks::now,
};
use flcrypto::tofrombytes::ToFromBytes;
use thiserror::Error;

use crate::{
    dht_router::kademlia::KNode,
    flo::{
        flo::{Flo, FloID},
        realm::{FloRealm, RealmID},
    },
};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RealmConfig {
    /// 16 exa bytes should be enough for everybody
    pub max_space: u64,
    pub max_flo_size: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct FloMeta {
    pub id: FloID,
    pub cuckoos: u32,
    pub version: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DHTConfig {
    // Realms allowed in this instance. If the Vec is empty, all new realms are
    // allowed.
    pub realms: Vec<RealmID>,
    // Flos owned by this instance, which will not be removed.
    pub owned: Vec<FloID>,
    // How long the get_ methods will wait before returning a timeout.
    pub timeout: u64,
}

impl Default for DHTConfig {
    fn default() -> Self {
        Self {
            realms: vec![],
            owned: vec![],
            timeout: 1000,
        }
    }
}

pub type FloCuckoo = (Flo, Vec<FloID>);

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct FloConfig {
    // Linking a Flo to a foreign flo - how long and to whom it links.
    pub cuckoo: Cuckoo,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum Cuckoo {
    Duration(u32),
    Parent(FloID),
    None,
}

impl Default for Cuckoo {
    fn default() -> Self {
        Cuckoo::None
    }
}

/// The DHTStorageCore structure holds a configuration and the storage
/// needed to persist over reloads of the node.
#[derive(VersionedSerde, Debug, Clone, PartialEq)]
pub struct RealmStorage {
    dht_config: DHTConfig,
    realm_config: RealmConfig,
    realm_id: RealmID,
    root: U256,
    flos: HashMap<FloID, FloStorage>,
    distances: HashMap<usize, Vec<FloID>>,
    size: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
struct FloStorage {
    flo: Flo,
    cuckoos: Vec<FloID>,
    time_create: i64,
    time_update: i64,
    time_read: i64,
    reads: i64,
}

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("No Flo with a domain stored")]
    DomainMissing,
    #[error("Domain without history")]
    DomainNoHistory,
    #[error("Domain is not a root domain")]
    DomainNotRoot,
    #[error("This realm is not accepted")]
    RealmNotAccepted,
}

impl RealmStorage {
    pub fn new(dht_config: DHTConfig, root: NodeID, realm: FloRealm) -> Result<Self, CoreError> {
        let realm_config = realm.cache().get_config();
        let realm_id = realm.flo().realm_id();
        let mut s = Self {
            dht_config,
            realm_config,
            realm_id,
            root,
            flos: HashMap::new(),
            distances: HashMap::new(),
            size: 0,
        };
        s.put(realm.flo().clone());
        Ok(s)
    }

    pub fn get_flo_cuckoo(&self, id: &FloID) -> Option<FloCuckoo> {
        self.flos
            .get(id)
            .map(|fs| (fs.flo.clone(), fs.cuckoos.clone()))
    }

    pub fn get_cuckoo_ids(&self, key: &FloID) -> Option<Vec<FloID>> {
        self.flos
            .get(key)
            .map(|dbs| dbs.cuckoos.iter().cloned().collect())
    }

    pub fn get_flo_metas(&self) -> Vec<FloMeta> {
        self.flos
            .values()
            .map(|df| FloMeta {
                id: df.flo.flo_id(),
                version: df.version(),
                cuckoos: df.cuckoos.len() as u32,
            })
            .collect()
    }

    pub fn store_cuckoo_id(&mut self, parent: &FloID, cuckoo: FloID) {
        self.flos
            .get_mut(parent)
            .map(|fs| (!fs.cuckoos.contains(&cuckoo)).then(|| fs.cuckoos.push(cuckoo)));
    }

    /// TODO: decide which IDs need to be stored.
    pub fn sync_available(&self, available: &Vec<FloMeta>) -> Option<Vec<FloID>> {
        let a: Vec<_> = available
            .iter()
            // This is actually correct, but perhaps not readable enough...
            .filter_map(|remote| {
                (self.flos.get(&remote.id).map(|local| {
                    local.flo.version() >= remote.version
                        && local.cuckoos.len() as u32 >= remote.cuckoos
                }) != Some(true))
                .then(|| remote.id.clone())
            })
            .collect();
        (a.len() > 0).then(|| a)
    }

    pub fn upsert_flo(&mut self, flo: Flo) -> bool {
        if flo.size() as u64 * 3 > self.realm_config.max_space {
            log::debug!(
                "Cannot store flo of size {} > max_space({}) / 3",
                flo.size(),
                self.realm_config.max_space
            );
            return false;
        }

        let mut updated = false;
        let flo_id = flo.flo_id();
        flo.flo_config()
            .cuckoo_parent()
            .map(|pid| self.store_cuckoo_id(pid, flo_id.clone()));

        if let Some(old) = self.flos.get(&flo.flo_id()) {
            if old.version() < flo.version() {
                self.put(flo);
                updated = true;
            }
        } else {
            self.put(flo);
            updated = true;
        }

        while self.size as u64 > self.realm_config.max_space {
            self.remove_furthest(&flo_id);
        }
        updated
    }

    // pub fn upsert_flo_cuckoo(&mut self, fc: FloCuckoo) -> bool {
    //     let fid = fc.0.flo_id();
    //     let updated = self.upsert_flo(fc.0);
    //     fc.1.into_iter()
    //         .for_each(|id| self.store_cuckoo_id(&fid, id));
    //     updated
    // }

    fn put(&mut self, flo: Flo) {
        let id = flo.flo_id();
        // log::trace!(
        //     "{} Storing {}/{}/{}",
        //     self.root,
        //     id,
        //     flo.version(),
        //     flo.flo_type()
        // );
        self.remove_entry(&id);
        let depth = KNode::get_depth(&self.root, *id);
        self.distances
            .entry(depth)
            .or_insert_with(Vec::new)
            .push(id.clone());
        let df: FloStorage = flo.into();
        self.size += df.size();
        self.flos.insert(id, df);
    }

    fn remove_entry(&mut self, id: &FloID) {
        if let Some(df) = self.flos.remove(id) {
            let distance = KNode::get_depth(&self.root, **id);
            self.distances
                .entry(distance)
                .and_modify(|v| v.retain(|i| i != id));
            self.size -= df.size();
        }
    }

    fn remove_furthest(&mut self, not_delete: &FloID) {
        if let Some(furthest) = self
            .distances
            .iter()
            .filter_map(|(dist, flos)| {
                if flos.contains(not_delete) {
                    flos.len() > 1
                } else {
                    flos.len() > 0
                }
                .then(|| dist)
            })
            .sorted()
            .unique()
            .next()
        {
            if let Some(id) = self
                .distances
                .get(furthest)
                .and_then(|ids| {
                    ids.iter()
                        .filter(|&id| id != not_delete)
                        .collect::<Vec<&FloID>>()
                        .first()
                        .cloned()
                })
                .cloned()
            {
                log::trace!(
                    "{}: Removing furthest {id}/{}",
                    self.root,
                    KNode::get_depth(&self.root, *id.clone())
                );

                self.remove_entry(&id);
            }
        }
    }
}

impl From<Flo> for FloStorage {
    fn from(flo: Flo) -> Self {
        Self {
            flo,
            cuckoos: vec![],
            time_create: now(),
            time_update: 0,
            time_read: 0,
            reads: 0,
        }
    }
}

impl FloStorage {
    fn version(&self) -> u32 {
        self.flo.version()
    }
}

impl DHTConfig {
    pub fn accepts_realm(&self, id: &RealmID) -> bool {
        self.realms.len() == 0 || self.realms.contains(id)
    }
}

impl FloConfig {
    pub fn allows_cuckoo(&self, age: u32) -> bool {
        match self.cuckoo {
            Cuckoo::Duration(t) => age < t,
            _ => false,
        }
    }

    pub fn is_cuckoo_of(&self, parent: &FloID) -> bool {
        match &self.cuckoo {
            Cuckoo::Parent(flo_id) => flo_id == parent,
            _ => false,
        }
    }

    pub fn cuckoo_parent(&self) -> Option<&FloID> {
        match &self.cuckoo {
            Cuckoo::Parent(flo_id) => Some(flo_id),
            _ => None,
        }
    }
}

/// Here you must write the necessary unit-tests to make sure that your core algorithms
/// work the way you want them to.
#[cfg(test)]
mod tests {
    use std::{error::Error, str::FromStr};

    use flarch::start_logging_filter_level;
    use flcrypto::access::Condition;

    use crate::flo::{
        blob::{Blob, FloBlob},
        crypto::Rules,
    };

    use super::*;

    // use crate::flo::testing::{new_ace, new_dht_flo_depth};
    // use flcrypto::access::Version;

    #[test]
    fn test_cuckoo() -> Result<(), Box<dyn Error>> {
        let root = U256::from_str("00").unwrap();
        let fr = FloRealm::new(
            "root",
            crate::flo::crypto::Rules::None,
            RealmConfig {
                max_space: 1000000,
                max_flo_size: 1000,
            },
        )?;
        let rid = fr.realm_id();
        let mut storage = RealmStorage::new(DHTConfig::default(), root.into(), fr.clone())?;

        let data = &("".to_string());
        let fp = Flo::new(rid.clone(), Rules::None, data, FloConfig::default())?;
        let fp_cuckoo = Flo::new(
            rid.clone(),
            Rules::None,
            data,
            FloConfig {
                cuckoo: Cuckoo::Parent(fp.flo_id()),
            },
        )?;
        storage.put(fp.clone().into());
        storage.put(fp_cuckoo.into());
        assert!(storage.get_cuckoo_ids(&fp.flo_id()).is_some());

        Ok(())
    }

    fn get_flo_depth(root: &NodeID, rid: &RealmID, depth: usize) -> Flo {
        loop {
            let flo =
                Flo::new(rid.clone(), Rules::None, &U256::rnd(), FloConfig::default()).unwrap();
            let nd = KNode::get_depth(root, *flo.flo_id());
            if nd == depth {
                return flo;
            }
        }
    }

    #[test]
    fn test_furthest() -> Result<(), Box<dyn Error>> {
        start_logging_filter_level(vec![], log::LevelFilter::Info);
        let root = U256::from_str("00").unwrap();
        let realm = FloRealm::new(
            "name",
            Rules::None,
            RealmConfig {
                max_space: 1e6 as u64,
                max_flo_size: 1e6 as u32,
            },
        )?;
        let rid = realm.realm_id();
        let mut storage = RealmStorage::new(DHTConfig::default(), root, realm)?;
        let _flos1: Vec<Flo> = (1..=3)
            .map(|i| get_flo_depth(&root, &rid, i))
            .inspect(|flo| storage.put(flo.clone()))
            .collect();

        assert_eq!(4, storage.distances.len());
        assert_eq!(4, storage.flos.len());
        let size = storage.size;

        let _flos2: Vec<Flo> = (1..=3)
            .map(|i| get_flo_depth(&root, &rid, i))
            .inspect(|flo| storage.put(flo.clone()))
            .collect();

        assert_eq!(4, storage.distances.len());
        assert_eq!(1, storage.distances.get(&0).unwrap().len());
        assert_eq!(2, storage.distances.get(&1).unwrap().len());
        assert_eq!(2, storage.distances.get(&2).unwrap().len());
        assert_eq!(2, storage.distances.get(&3).unwrap().len());
        assert_eq!(7, storage.flos.len());
        assert!(storage.size > size);
        let size = storage.size;

        storage.remove_furthest(&root.into());
        assert_eq!(4, storage.distances.len());
        assert_eq!(0, storage.distances.get(&0).unwrap().len());
        assert_eq!(2, storage.distances.get(&1).unwrap().len());
        assert_eq!(2, storage.distances.get(&2).unwrap().len());
        assert_eq!(2, storage.distances.get(&3).unwrap().len());
        assert_eq!(6, storage.flos.len());
        assert!(storage.size < size);
        let size = storage.size;

        storage.remove_furthest(&root.into());
        assert_eq!(4, storage.distances.len());
        assert_eq!(0, storage.distances.get(&0).unwrap().len());
        assert_eq!(1, storage.distances.get(&1).unwrap().len());
        assert_eq!(2, storage.distances.get(&2).unwrap().len());
        assert_eq!(2, storage.distances.get(&3).unwrap().len());
        assert_eq!(5, storage.flos.len());
        assert!(storage.size < size);

        storage.remove_furthest(&root.into());
        assert_eq!(4, storage.distances.len());
        assert_eq!(0, storage.distances.get(&0).unwrap().len());
        assert_eq!(0, storage.distances.get(&1).unwrap().len());
        assert_eq!(2, storage.distances.get(&2).unwrap().len());
        assert_eq!(2, storage.distances.get(&3).unwrap().len());
        assert_eq!(4, storage.flos.len());
        assert!(storage.size < size);

        Ok(())
    }

    #[test]
    fn test_update() -> Result<(), Box<dyn Error>> {
        start_logging_filter_level(vec![], log::LevelFilter::Info);

        let root = U256::from_str("00").unwrap();
        let realm = FloRealm::new(
            "name",
            Rules::None,
            RealmConfig {
                max_space: 1e6 as u64,
                max_flo_size: 1e6 as u32,
            },
        )?;
        let rid = realm.realm_id();
        let mut storage = RealmStorage::new(DHTConfig::default(), root, realm)?;

        let mut fw = FloBlob::from_type(
            rid.clone(),
            Rules::Update(Condition::Pass),
            Blob::new("test"),
        )?;
        storage.put(fw.flo().clone());

        let fid = fw.flo_id();
        assert_eq!(
            Some(vec![fid.clone()]),
            storage.sync_available(&vec![FloMeta {
                id: fid.clone(),
                version: 1,
                cuckoos: 0,
            }])
        );
        fw.edit_sign_update(
            |b| b.set_path("path"),
            Condition::Pass,
            Rules::Update(Condition::Pass),
            vec![],
            &[],
        )?;

        assert!(storage.upsert_flo(fw.into()));
        assert_eq!(
            None,
            storage.sync_available(&vec![FloMeta {
                id: fid.clone(),
                version: 1,
                cuckoos: 0,
            }])
        );

        Ok(())
    }
}
