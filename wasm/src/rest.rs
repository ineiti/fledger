use common::config::NodeInfo;
use common::rest::{RestCaller,RestClient};

use wasm_bindgen::prelude::*;

/// Runs all calls to test the REST interface
pub async fn demo() -> Result<(), JsValue> {
    console_log!("Starting REST-test in 2021");
    let rc = RestCaller::new("http://localhost:8000");
    let mut rest = RestClient::new(rc);
    rest.clear_nodes().await?;
    let id1 = rest.new_id().await?;
    let ni1 = NodeInfo::new();
    rest.add_id(id1, &ni1).await?;
    console_log!("Added new node");
    console_log!("IDs: {:?}", rest.list_ids().await?);

    Ok(())
}
