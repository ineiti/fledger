use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

use super::flo::FloWrapper;

pub type Blob = FloWrapper<BlobData>;

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BlobData {
    #[serde_as(as = "Base64")]
    pub data: Bytes,
    pub content: String,
}

impl Blob {}
