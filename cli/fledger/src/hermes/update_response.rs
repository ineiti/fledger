use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, Default)]
pub struct UpdateResponse {
    pub target_page_ids: Option<Vec<String>>,
}

impl UpdateResponse {}
