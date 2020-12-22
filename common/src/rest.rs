use serde::{Deserialize,Serialize};
use crate::{config::NodeInfo, types::U256};

#[derive(Deserialize, Serialize)]
pub struct PostWebRTC {
    pub list_id: U256,
    pub node: NodeInfo,
}

#[derive(Deserialize, Serialize)]
pub struct GetListID {
    pub new_id: U256,
}

#[derive(Deserialize, Serialize)]
pub struct GetWebRTC {
    pub list: Box<[PostWebRTC]>,
}
