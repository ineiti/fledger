use common::node::ext_interface::{DataStorage,Logger};

use web_sys::window;

pub struct LocalStorage {}

impl DataStorage for LocalStorage {
    fn load(&self, key: &str) -> Result<String, String> {
        window()
            .unwrap()
            .local_storage()
            .map_err(|e| e.as_string().unwrap())?
            .unwrap()
            .get(key)
            .map(|s| s.unwrap_or("".to_string()))
            .map_err(|e| e.as_string().unwrap())
    }

    fn save(&self, key: &str, value: &str) -> Result<(), String> {
        window()
            .unwrap()
            .local_storage()
            .map_err(|e| e.as_string().unwrap())?
            .unwrap()
            .set(key, value)
            .map_err(|e| e.as_string().unwrap())
    }
}

pub struct ConsoleLogger {}

impl Logger for ConsoleLogger {
    fn info(&self, s: &str) {
        console_log!("info: {}", s);
    }

    fn warn(&self, s: &str) {
        console_warn!("warn: {}", s);
    }

    fn error(&self, s: &str) {
        console_warn!(" err: {}", s);
    }

    fn clone(&self) -> Box<dyn Logger> {
        Box::new(ConsoleLogger {})
    }
}
