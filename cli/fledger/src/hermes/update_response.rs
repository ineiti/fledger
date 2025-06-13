use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, Default)]
pub struct UpdateResponse {
    pub target_page_ids: Vec<String>,
}

impl UpdateResponse {}
