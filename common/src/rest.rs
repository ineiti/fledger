use serde::{Deserialize, Serialize};
use std::future::Future;

use crate::ext_interface::RestCall;

use crate::ext_interface::RestMethod;use crate::{config::NodeInfo, types::U256};

#[derive(Debug, Deserialize, Serialize)]
pub struct PostWebRTC {
    pub list_id: U256,
    pub node: NodeInfo,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GetListID {
    pub new_id: U256,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GetWebRTC {
    pub list: Vec<NodeInfo>,
}

pub struct RestClient<'r, F>
where
    F: Future,
{
    rest: RestCall<'r, F>,
}

impl<'r, F> RestClient<'r, F>
where
    F: Future<Output = Result<String, String>>,
{
    pub fn new(rest: RestCall<'r, F>) -> RestClient<'r, F> {
        RestClient { rest }
    }

    pub async fn clear_nodes(&self) -> Result<(), String> {
        (self.rest)(RestMethod::DELETE, "clearNodes", None)
            .await
            .map(|_| ())
    }

    pub async fn new_id(&mut self) -> Result<U256, String> {
        let reply = (self.rest)(RestMethod::GET, "newID", None).await?;
        let id: GetListID = serde_json::from_str(reply.as_str()).map_err(|e| e.to_string())?;
        Ok(id.new_id)
    }

    pub async fn add_id(&mut self, id: U256, ni: NodeInfo) -> Result<(), String> {
        let pw = serde_json::to_string(&PostWebRTC {
            list_id: id,
            node: ni,
        })
        .map_err(|e| e.to_string())?;
        (self.rest)(RestMethod::POST, "addNode".clone(), Some(pw))
            .await
            .map(|_| ())
    }

    pub async fn list_ids(&mut self) -> Result<Vec<NodeInfo>, String> {
        let reply = (self.rest)(RestMethod::GET, "listIDs", None).await?;
        let ids: GetWebRTC = serde_json::from_str(reply.as_str()).map_err(|e| e.to_string())?;
        Ok(ids.list)
    }
}
