use std::collections::HashMap;

use bytes::Bytes;
use flarch::nodeids::U256;
use flcrypto::{access::Condition, signer::Signer};
use flmacro::AsU256;
use serde::{Deserialize, Serialize};

use crate::dht_storage::core::Cuckoo;

use super::{flo::FloWrapper, realm::RealmID};

#[derive(AsU256, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
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
        cond: Condition,
        path: &str,
        index: Bytes,
        parent: Option<BlobID>,
        signers: &[&Signer],
    ) -> anyhow::Result<Self> {
        Self::new_cuckoo(realm, cond, path, index, parent, Cuckoo::None, signers)
    }

    pub fn new_cuckoo(
        realm: RealmID,
        cond: Condition,
        path: &str,
        index: Bytes,
        parent: Option<BlobID>,
        cuckoo: Cuckoo,
        signers: &[&Signer],
    ) -> anyhow::Result<Self> {
        let links = parent
            .map(|p| [("parent".to_string(), vec![p])].into_iter().collect())
            .unwrap_or(HashMap::new());
        Self::from_type_cuckoo(
            realm,
            cond,
            cuckoo,
            BlobPage(Blob {
                blob_type: "re.fledg.page".into(),
                links,
                datas: [("index.html".to_string(), index)].into(),
                values: [("path".to_string(), path.into())].into(),
            }),
            signers,
        )
    }

    pub fn get_index(&self) -> String {
        self.cache()
            .0
            .datas
            .get("index.html")
            .map(|b| String::from_utf8(b.to_vec()).unwrap_or_default())
            .unwrap_or("".into())
    }

    pub fn blob_id(&self) -> BlobID {
        (*self.flo_id()).into()
    }
}

impl FloBlobTag {
    pub fn new(
        realm: RealmID,
        cond: Condition,
        name: &str,
        parent: Option<BlobID>,
        signers: &[&Signer],
    ) -> anyhow::Result<Self> {
        Self::new_cuckoo(realm, cond, name, parent, Cuckoo::None, signers)
    }

    pub fn new_cuckoo(
        realm: RealmID,
        cond: Condition,
        name: &str,
        parent: Option<BlobID>,
        cuckoo: Cuckoo,
        signers: &[&Signer],
    ) -> anyhow::Result<Self> {
        let links = parent
            .map(|p| [("parent".to_string(), vec![p])].into_iter().collect())
            .unwrap_or(HashMap::new());
        Self::from_type_cuckoo(
            realm,
            cond,
            cuckoo,
            BlobTag(Blob {
                blob_type: "re.fledg.tag".into(),
                links,
                datas: HashMap::new(),
                values: [("name".to_string(), name.into())].into(),
            }),
            signers,
        )
    }

    pub fn blob_id(&self) -> BlobID {
        (*self.flo_id()).into()
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

    pub fn get_parents(&mut self) -> Option<&Vec<BlobID>> {
        self.links.get("parent")
    }

    pub fn get_path(&mut self) -> Option<&String> {
        self.values.get("path")
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
    use flcrypto::access::Condition;

    use crate::{flo::crypto::FloBadge, testing::wallet::Wallet};

    use super::*;

    #[test]
    fn test_update() -> anyhow::Result<()> {
        let mut wallet = Wallet::new();
        FloBadge::from_type(RealmID::rnd(), Condition::Fail, wallet.get_badge(), &[])?;
        let flb = FloBlobPage::new(
            RealmID::rnd(),
            Condition::Verifier(wallet.get_verifier()),
            "",
            Bytes::from(""),
            None,
            &[&wallet.get_signer()],
        )?;

        let flb2 = flb.edit_data_signers(
            Condition::Verifier(wallet.get_verifier()),
            |bp| bp.0.set_parents(vec![BlobID::rnd()]),
            &[&mut wallet.get_signer()],
        )?;

        assert_eq!(1, flb2.version());

        let flb3 = flb2.edit_data_signers(
            Condition::Verifier(wallet.get_verifier()),
            |bp| {
                bp.0.datas
                    .insert("index.html".into(), "<html><h1>Root</h1></html>".into());
            },
            &[&mut wallet.get_signer()],
        )?;

        assert_eq!(2, flb3.version());

        Ok(())
    }
}
