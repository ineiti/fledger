pub trait Storage {
    fn load(&self, key: &str) -> Result<String, &'static str>;

    fn save(&self, key: &str, value: &str) -> Result<bool, &'static str>;
}

pub trait Logger {
    fn log(&self, s: &str);
    fn warn(&self, s: &str);
    fn error(&self, s: &str);
}
