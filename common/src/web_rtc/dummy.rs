use super::WebRTCCallerState;
use super::WebRTCMethod;

pub struct WebRTCCaller {}

impl WebRTCCaller {
    pub fn new(_: WebRTCCallerState) -> Result<WebRTCCaller, String> {
        Ok(WebRTCCaller {})
    }

    pub async fn call(
        &mut self,
        _: WebRTCMethod,
        _: Option<String>,
    ) -> Result<Option<String>, String> {
        Err("Not implemented".to_string())
    }
}
