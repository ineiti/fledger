use flmodules::flo::{flo, realm};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash, Eq)]
pub struct RealmID(realm::RealmID);

impl RealmID {
    pub fn new(id: realm::RealmID) -> RealmID {
        RealmID(id)
    }

    pub fn get_id(&self) -> realm::RealmID {
        self.0.clone()
    }
}

#[wasm_bindgen]
impl RealmID {
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string(&self) -> String {
        format!("{}", self.0)
    }
}

impl From<realm::RealmID> for RealmID {
    fn from(id: realm::RealmID) -> Self {
        RealmID(id)
    }
}

impl Into<realm::RealmID> for RealmID {
    fn into(self) -> realm::RealmID {
        self.0
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FloID(flo::FloID);

impl FloID {
    pub fn new(id: flo::FloID) -> FloID {
        FloID(id)
    }

    pub fn get_id(&self) -> flo::FloID {
        self.0.clone()
    }
}

#[wasm_bindgen]
impl FloID {
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string(&self) -> String {
        format!("{}", self.0)
    }
}

impl From<flo::FloID> for FloID {
    fn from(id: flo::FloID) -> Self {
        FloID(id)
    }
}

impl Into<flo::FloID> for FloID {
    fn into(self) -> flo::FloID {
        self.0
    }
}
