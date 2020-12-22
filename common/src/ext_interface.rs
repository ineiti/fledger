pub trait Network {
    fn listen(&self);

    fn close(&self);

    fn send(&self);
}

pub trait Storage {
    fn load(&self, key: &str) -> Result<String, &'static str>;

    fn save(&self, key: str, value: str) -> Result<bool, &'static str>;
}
