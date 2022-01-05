/// This is the common module trait that every module needs to implement.
trait Module {
    fn new(ds: dyn DataStorage) -> Self;

    fn process_message() -> Vec<Message>;

    fn tick() -> Vec<Message>;
}

/// The DataStorage trait allows access to a persistent storage. Each module
/// has it's own DataStorage, so there will never be a name clash.
trait DataStorage {
    fn get(&self, key: &str) -> String;

    fn put(&mut self, key: &str, value: &str);
}
