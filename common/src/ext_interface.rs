pub type RestCall<'r, F> = fn(method: RestMethod, path: &'r str, data: Option<String>) -> F;

pub enum RestMethod {
    GET,
    POST,
    PUT,
    DELETE,
}

pub type WebRTCCall<F> = fn(input: Option<String>) -> F;

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

pub trait Storage {
    fn load(&self, key: &str) -> Result<String, &'static str>;

    fn save(&self, key: &str, value: &str) -> Result<bool, &'static str>;
}

pub trait Logger {
    fn log(&self, s: &str);
    fn warn(&self, s: &str);
    fn error(&self, s: &str);
}
