use async_trait::async_trait;

pub trait DataStorage {
    fn load(&self, key: &str) -> Result<String, String>;

    fn save(&self, key: &str, value: &str) -> Result<(), String>;
}

pub trait Logger {
    fn info(&self, s: &str);
    fn warn(&self, s: &str);
    fn error(&self, s: &str);
}

pub enum RestMethod {
    Get,
    Post,
    Put,
    Delete,
}

#[async_trait(?Send)]
pub trait RestCaller {
    async fn call(
        &self,
        method: RestMethod,
        path: String,
        data: Option<String>,
    ) -> Result<String, String>;
}

#[async_trait(?Send)]
pub trait WebRTCCaller {
    async fn call(
        &mut self,
        call: WebRTCMethod,
        input: Option<String>,
    ) -> Result<Option<String>, String>;
}

pub enum WebRTCMethod {
    MakeOffer,
    MakeAnswer,
    UseAnswer,
    WaitGathering,
    IceString,
    IcePut,
    MsgReceive,
    MsgSend,
    PrintStates,
}

/// What type of node this is
#[derive(PartialEq, Debug)]
pub enum WebRTCCallerState {
    Initializer,
    Follower,
}
