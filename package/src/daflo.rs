//! wasm_bindgen compatible versions of FloWrappers.

use flmodules::flo::{blob, flo};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloBlobPage(blob::FloBlobPage);

#[wasm_bindgen]
impl FloBlobPage {
    pub fn get_index(&self) -> String {
        self.0.get_index()
    }
}

impl TryFrom<flo::Flo> for FloBlobPage {
    type Error = anyhow::Error;

    fn try_from(value: flo::Flo) -> Result<Self, Self::Error> {
        Ok(FloBlobPage(blob::FloBlobPage::try_from(value)?))
    }
}
