use async_trait::async_trait;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use web_sys::{Request, RequestInit, RequestMode, Response};

pub struct RestCallerWasm {
    base: String,
}

impl RestCallerWasm {
    pub fn new(base: &str) -> RestCallerWasm {
        RestCallerWasm {
            base: base.to_string(),
        }
    }
}

#[async_trait(?Send)]
impl RestCaller for RestCallerWasm {
    async fn call(
        &self,
        method: RestMethod,
        path: String,
        data: Option<String>,
    ) -> Result<String, String> {
        let mut opts = RequestInit::new();
        opts.method(match method {
            RestMethod::Get => "GET",
            RestMethod::Put => "PUT",
            RestMethod::Delete => "DELETE",
            RestMethod::Post => "POST",
        });
        opts.mode(RequestMode::Cors);
        if data.is_some() {
            opts.body(Some(&JsValue::from_str(&data.unwrap())));
        }
        let url = format!("{}/{}", self.base, path);
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
}
