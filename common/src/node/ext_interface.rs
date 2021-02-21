pub trait DataStorage {
    fn load(&self, key: &str) -> Result<String, String>;

    fn save(&self, key: &str, value: &str) -> Result<(), String>;
}

pub trait Logger: Send {
    fn info(&self, s: &str);
    fn warn(&self, s: &str);
    fn error(&self, s: &str);
    fn clone(&self) -> Box<dyn Logger>;
}
