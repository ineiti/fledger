use common::config::NodeInfo;

use common::ext_interface::RestMethod;

use common::rest::RestClient;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use web_sys::{Request, RequestInit, RequestMode, Response};

pub async fn rest_func(
    method: RestMethod,
    path: &str,
    data: Option<String>,
) -> Result<String, String> {
    let mut opts = RequestInit::new();
    opts.method(match method {
        RestMethod::GET => "GET",
        RestMethod::PUT => "PUT",
        RestMethod::DELETE => "DELETE",
        RestMethod::POST => "POST",
    });
    opts.mode(RequestMode::Cors);
    if data.is_some() {
        opts.body(Some(&JsValue::from_str(&data.unwrap())));
    }
    let url = format!("{}/{}", "http://localhost:8000", path);
    let request =
        Request::new_with_str_and_init(&url, &opts).map_err(|e| e.as_string().unwrap())?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| e.as_string().unwrap())?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| e.as_string().unwrap())?;

    let window = web_sys::window().unwrap();
    match JsFuture::from(window.fetch_with_request(&request)).await {
        Ok(res) => {
            if !res.is_instance_of::<Response>() {
                return Err("Didn't get Response instance".to_string());
            }
            let resp: Response = res.into();
            let text = resp.text().unwrap();
            match JsFuture::from(text).await {
                Ok(s) => Ok(s.as_string().unwrap()),
                Err(_) => Err("something went wrong".to_string()),
            }
        }
        Err(_) => Err("something else went wrong".to_string()),
    }
}

/// Runs all calls to test the REST interface
pub async fn demo() -> Result<(), JsValue> {
    // let base = "http://localhost:8000";
    console_log!("Starting REST-test");
    let mut rest = RestClient::new(rest_func);
    rest.clear_nodes().await?;
    let id1 = rest.new_id().await?;
    let ni1 = NodeInfo::new();
    rest.add_id(id1, ni1).await?;
    console_log!("Added new node");
    console_log!("IDs: {:?}", rest.list_ids().await?);

    Ok(())
}
