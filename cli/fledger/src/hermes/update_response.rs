use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, Default)]
pub struct UpdateResponse {
    pub target_page_id: Option<String>,
    pub timed_data_points: Vec<u32>,
    pub timeless_data_points: Vec<u32>,
}

impl UpdateResponse {}
