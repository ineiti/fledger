use super::RestMethod;

pub struct RestCaller {}

impl RestCaller {
    pub fn new(_: &str)-> RestCaller{
        RestCaller{}
    }

    pub async fn call(
        &self,
        _: RestMethod,
        _: String,
        _: Option<String>,
    ) -> Result<String, String> {
        Err("Not implemented".to_string())
    }
}
