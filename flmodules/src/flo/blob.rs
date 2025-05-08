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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Blob {
    blob_type: String,
    links: HashMap<String, Vec<BlobID>>,
    values: HashMap<String, String>,
    datas: HashMap<String, Bytes>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct BlobPage(pub Blob);

pub type FloBlobPage = FloWrapper<BlobPage>;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct BlobTag(pub Blob);

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
        self.get_data("index.html")
            .map(|b| String::from_utf8(b.to_vec()).unwrap_or_default())
            .unwrap_or("".into())
    }

    pub fn blob_id(&self) -> BlobID {
        (*self.flo_id()).into()
    }
}

impl std::fmt::Display for FloBlobPage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("  {}\n", self.flo_id()))?;
        f.write_fmt(format_args!(
            "    {}\n",
            self.get_path()
                .map(|p| format!("path: {p}"))
                .unwrap_or("no path".into())
        ))?;
        f.write_fmt(format_args!(
            "    parents: {}\n",
            self.get_parents()
                .iter()
                .map(|p| format!("{p}"))
                .collect::<Vec<_>>()
                .join(", ")
        ))?;
        f.write_fmt(format_args!("    version: {}\n", self.version()))?;
        f.write_fmt(format_args!(
            "    children: {}\n",
            self.get_children()
                .iter()
                .map(|p| format!("{p}"))
                .collect::<Vec<_>>()
                .join(", ")
        ))?;
        f.write_fmt(format_args!(
            "    files:\n{}",
            self.datas()
                .iter()
                .map(|(k, v)| format!("      {k} -> {} B", v.len()))
                .collect::<Vec<_>>()
                .join("\n")
        ))
    }
}

impl BlobAccess for FloBlobPage {
    fn get_blob(&self) -> &Blob {
        &self.cache().0
    }

    fn get_blob_mut(&mut self) -> &mut Blob {
        &mut self.cache_mut().0
    }
}

impl BlobFamily for FloBlobPage {}
impl BlobPath for FloBlobPage {}

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

impl BlobAccess for FloBlobTag {
    fn get_blob(&self) -> &Blob {
        &self.cache().0
    }

    fn get_blob_mut(&mut self) -> &mut Blob {
        &mut self.cache_mut().0
    }
}

impl BlobFamily for FloBlobTag {}
impl BlobPath for FloBlobTag {}

impl Blob {
    pub fn new(blob_type: &str) -> Self {
        Self {
            blob_type: blob_type.to_string(),
            links: HashMap::new(),
            values: HashMap::new(),
            datas: HashMap::new(),
        }
    }
}

impl BlobFamily for FloBlob {}
impl BlobPath for FloBlob {}

impl BlobAccess for FloBlob {
    fn get_blob(&self) -> &Blob {
        self.cache()
    }

    fn get_blob_mut(&mut self) -> &mut Blob {
        self.cache_mut()
    }
}

pub trait BlobAccess {
    fn get_blob(&self) -> &Blob;
    fn get_blob_mut(&mut self) -> &mut Blob;

    fn links(&self) -> &HashMap<String, Vec<BlobID>> {
        &self.get_blob().links
    }

    fn values(&self) -> &HashMap<String, String> {
        &self.get_blob().values
    }

    fn datas(&self) -> &HashMap<String, Bytes> {
        &self.get_blob().datas
    }

    fn get_data(&self, key: &str) -> Option<&Bytes> {
        self.datas().get(key)
    }

    fn set_data(&mut self, key: String, value: Bytes) {
        self.datas_mut().insert(key, value);
    }

    fn links_mut(&mut self) -> &mut HashMap<String, Vec<BlobID>> {
        &mut self.get_blob_mut().links
    }

    fn values_mut(&mut self) -> &mut HashMap<String, String> {
        &mut self.get_blob_mut().values
    }

    fn datas_mut(&mut self) -> &mut HashMap<String, Bytes> {
        &mut self.get_blob_mut().datas
    }
}

pub trait BlobFamily: BlobAccess {
    fn set_parents(&mut self, parents: Vec<BlobID>) {
        self.links_mut().insert("parents".into(), parents);
    }
    fn add_parent(&mut self, parent: BlobID) {
        self.links_mut()
            .entry("parents".into())
            .or_insert_with(Vec::new)
            .push(parent);
    }
    fn get_parents(&self) -> Vec<BlobID> {
        self.links().get("parents").unwrap_or(&vec![]).clone()
    }
    fn set_children(&mut self, children: Vec<BlobID>) {
        self.links_mut().insert("children".into(), children);
    }
    fn add_child(&mut self, child: BlobID) {
        self.links_mut()
            .entry("children".into())
            .or_insert_with(Vec::new)
            .push(child);
    }
    fn get_children(&self) -> Vec<BlobID> {
        self.links().get("children").unwrap_or(&vec![]).clone()
    }
}

pub trait BlobPath: BlobAccess {
    fn set_path(&mut self, path: String) {
        self.values_mut().insert("path".into(), path);
    }
    fn get_path(&self) -> Option<&String> {
        self.values().get("path")
    }
}

impl BlobAccess for Blob {
    fn get_blob(&self) -> &Blob {
        &self
    }

    fn get_blob_mut(&mut self) -> &mut Blob {
        self
    }
}

impl BlobAccess for BlobPage {
    fn get_blob(&self) -> &Blob {
        &self.0
    }

    fn get_blob_mut(&mut self) -> &mut Blob {
        &mut self.0
    }
}

impl BlobAccess for BlobTag {
    fn get_blob(&self) -> &Blob {
        &self.0
    }

    fn get_blob_mut(&mut self) -> &mut Blob {
        &mut self.0
    }
}

impl BlobPath for Blob {}
impl BlobFamily for Blob {}

impl BlobPath for BlobPage {}
impl BlobFamily for BlobPage {}

impl BlobPath for BlobTag {}
impl BlobFamily for BlobTag {}

pub trait BlobPathFamily: BlobPath + BlobFamily {}

impl<T: BlobPath + BlobFamily> BlobPathFamily for T {}

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
            |bp| bp.set_parents(vec![BlobID::rnd()]),
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
