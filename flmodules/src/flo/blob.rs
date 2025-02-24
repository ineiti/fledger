use std::collections::HashMap;

use bytes::Bytes;
use flarch::nodeids::U256;
use flmacro::AsU256;
use serde::{Deserialize, Serialize};

use crate::dht_storage::core::Cuckoo;

use super::{
    crypto::Rules,
    flo::{FloError, FloWrapper},
    realm::RealmID,
};

#[derive(AsU256, Serialize, Deserialize, Clone)]
pub struct BlobID(U256);

pub type FloBlob = FloWrapper<Blob>;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Blob {
    blob_type: String,
    links: HashMap<String, Vec<BlobID>>,
    values: HashMap<String, String>,
    datas: HashMap<String, Bytes>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BlobPage(Blob);

pub type FloBlobPage = FloWrapper<BlobPage>;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BlobTag(Blob);

pub type FloBlobTag = FloWrapper<BlobTag>;

impl FloBlobPage {
    pub fn new(
        realm: RealmID,
        rules: Rules,
        path: &str,
        index: Bytes,
        parent: Option<BlobID>,
    ) -> Result<Self, FloError> {
        Self::new_cuckoo(realm, rules, path, index, parent, Cuckoo::None)
    }

    pub fn new_cuckoo(
        realm: RealmID,
        rules: Rules,
        path: &str,
        index: Bytes,
        parent: Option<BlobID>,
        cuckoo: Cuckoo,
    ) -> Result<Self, FloError> {
        let links = parent
            .map(|p| [("parent".to_string(), vec![p])].into_iter().collect())
            .unwrap_or(HashMap::new());
        Self::from_type_cuckoo(
            realm,
            rules,
            cuckoo,
            BlobPage(Blob {
                blob_type: "re.fledg.page".into(),
                links,
                datas: [("index.html".to_string(), index)].into(),
                values: [("path".to_string(), path.into())].into(),
            }),
        )
    }
}

impl FloBlobTag {
    pub fn new(
        realm: RealmID,
        rules: Rules,
        name: &str,
        parent: Option<BlobID>,
    ) -> Result<Self, FloError> {
        Self::new_cuckoo(realm, rules, name, parent, Cuckoo::None)
    }

    pub fn new_cuckoo(
        realm: RealmID,
        rules: Rules,
        name: &str,
        parent: Option<BlobID>,
        cuckoo: Cuckoo,
    ) -> Result<Self, FloError> {
        let links = parent
            .map(|p| [("parent".to_string(), vec![p])].into_iter().collect())
            .unwrap_or(HashMap::new());
        Self::from_type_cuckoo(
            realm,
            rules,
            cuckoo,
            BlobTag(Blob {
                blob_type: "re.fledg.tag".into(),
                links,
                datas: HashMap::new(),
                values: [("name".to_string(), name.into())].into(),
            }),
        )
    }
}

impl Blob {
    pub fn new(blob_type: &str) -> Self {
        Self {
            blob_type: blob_type.to_string(),
            links: HashMap::new(),
            values: HashMap::new(),
            datas: HashMap::new(),
        }
    }
    pub fn set_parents(&mut self, parents: Vec<BlobID>) {
        self.links.insert("parent".into(), parents);
    }

    pub fn set_path(&mut self, path: &str) {
        self.values.insert("path".into(), path.to_string());
    }
}

pub trait BlobAccess {
    fn links(&self) -> &HashMap<String, Vec<BlobID>>;
    fn values(&self) -> &HashMap<String, String>;
    fn datas(&self) -> &HashMap<String, Bytes>;
    fn links_mut(&mut self) -> &mut HashMap<String, Vec<BlobID>>;
    fn values_mut(&mut self) -> &mut HashMap<String, String>;
    fn datas_mut(&mut self) -> &mut HashMap<String, Bytes>;
}

pub trait BlobParents: BlobAccess {
    fn set_parents(&mut self, parents: Vec<BlobID>) {
        self.links_mut().insert("parents".into(), parents);
    }
    fn get_parents(&mut self) -> Vec<BlobID> {
        self.links().get("parents").unwrap_or(&vec![]).clone()
    }
}

impl BlobPage {
    pub fn set_parents(&mut self, parents: Vec<BlobID>) {
        self.0.set_parents(parents);
    }

    pub fn set_path(&mut self, path: &str) {
        self.0.set_path(path);
    }

    pub fn set_data(&mut self, file: &str, data: Bytes) {
        self.0.datas.insert(file.into(), data);
    }
}

impl BlobAccess for BlobPage {
    fn links(&self) -> &HashMap<String, Vec<BlobID>> {
        &self.0.links
    }

    fn values(&self) -> &HashMap<String, String> {
        &self.0.values
    }

    fn datas(&self) -> &HashMap<String, Bytes> {
        &self.0.datas
    }

    fn links_mut(&mut self) -> &mut HashMap<String, Vec<BlobID>> {
        &mut self.0.links
    }

    fn values_mut(&mut self) -> &mut HashMap<String, String> {
        &mut self.0.values
    }

    fn datas_mut(&mut self) -> &mut HashMap<String, Bytes> {
        &mut self.0.datas
    }
}

impl BlobParents for BlobPage {}

impl BlobTag {
    pub fn set_parents(&mut self, parents: Vec<BlobID>) {
        self.0.set_parents(parents);
    }

    pub fn set_path(&mut self, path: &str) {
        self.0.set_path(path);
    }

    pub fn set_services(&mut self, services: Vec<BlobID>) {
        self.0.links.insert("services".into(), services);
    }
}

#[cfg(test)]
mod test {
    use std::error::Error;

    use flcrypto::access::Condition;

    use crate::{flo::crypto::FloBadge, testing::flo::Wallet};

    use super::*;

    #[test]
    fn test_update() -> Result<(), Box<dyn Error>> {
        let mut wallet = Wallet::new();
        let badge = FloBadge::from_type(RealmID::rnd(), Rules::None, wallet.get_badge())?;
        let mut flb = FloBlobPage::new(
            RealmID::rnd(),
            Rules::Update(Condition::Verifier(wallet.get_verifier().get_id())),
            "",
            Bytes::from(""),
            None,
        )?;
        flb.edit_sign_update(
            |bp| bp.0.set_parents(vec![BlobID::rnd()]),
            Condition::Verifier(wallet.get_verifier().get_id()),
            flb.rules().clone(),
            vec![Box::new(badge.clone())],
            &[&*wallet.get_signer()],
        )?;

        assert_eq!(1, flb.version());

        flb.edit_sign_update(
            |bp| {
                bp.0.datas
                    .insert("index.html".into(), "<html><h1>Root</h1></html>".into());
            },
            Condition::Verifier(wallet.get_verifier().get_id()),
            flb.rules().clone(),
            vec![Box::new(badge.clone())],
            &[&*wallet.get_signer()],
        )?;

        assert_eq!(2, flb.version());

        Ok(())
    }
}
