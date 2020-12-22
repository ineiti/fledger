

use super::logs;

use common::config::NodeInfo;
use common::rest::GetListID;

use common::rest::PostWebRTC;
use common::types::U256;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use web_sys::{Request, RequestInit, RequestMode, Response};

pub async fn do_test() -> Result<(), JsValue>{
    /*
     * Connect to the signaling serve to fetch a new id
     */
    let base = "http://localhost:8000";
    clear_nodes(base).await?;
    let id1 = new_id(base).await?;
    let ni1 = NodeInfo::new();
    add_id(base, id1.new_id, ni1).await?;
    console_log!("Added new node");

    Ok(())
}

async fn clear_nodes(base: &str) -> Result<(), JsValue> {
    let mut opts = RequestInit::new();
    opts.method("DELETE");
    opts.mode(RequestMode::Cors);
    let url = format!("{}/clearNodes", base);
    let request = Request::new_with_str_and_init(&url, &opts)?;
    let window = web_sys::window().unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    assert!(resp_value.is_instance_of::<Response>());
    return Ok(());
}

async fn new_id(base: &str) -> Result<GetListID, JsValue> {
    let mut opts = RequestInit::new();
    opts.method("GET");
    opts.mode(RequestMode::Cors);
    let url = format!("{}/newID", base);
    let request = Request::new_with_str_and_init(&url, &opts)?;
    request.headers().set("Accept", "application/json")?;
    let window = web_sys::window().unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    assert!(resp_value.is_instance_of::<Response>());
    let resp: Response = resp_value.dyn_into().unwrap();
    let json = JsFuture::from(resp.json()?).await?;
    let new_id: GetListID = json.into_serde().unwrap();
    return Ok(new_id);
}

async fn add_id(base: &str, id: U256, ni: NodeInfo) -> Result<(), JsValue> {
    let mut opts = RequestInit::new();
    opts.method("POST");
    opts.mode(RequestMode::Cors);
    let pw = match serde_json::to_string(&PostWebRTC {
        list_id: id,
        node: ni,
    }) {
        Ok(j) => j,
        Err(e) => return Err(JsValue::from_str(e.to_string().as_str())),
    };
    opts.body(Some(&JsValue::from_str(&pw)));
    let url = format!("{}/addNode", base);
    let request = Request::new_with_str_and_init(&url, &opts)?;
    // request.headers().set("CT", "application/json")?;
    request.headers().delete("Content-Type")?;
    request
        .headers()
        .append("Content-Type", "application/json")?;
    request.headers().set("Accept", "application/json")?;
    let window = web_sys::window().unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    assert!(resp_value.is_instance_of::<Response>());
    return Ok(());
}
