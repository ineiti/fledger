use common::types::U256;

pub mod text_message;
use text_message::*;

const MESSAGE_MAXIMUM: usize = 20;

/// The first module to use the random_connections is a copy of the previous
/// chat.
/// It makes sure to update from the previous chat messages, and simply
/// copies all messages to all nodes.
/// It keeps 20 messages in memory, each message not being bigger than 1kB.
#[derive(Debug)]
pub struct Module {
    storage: TextMessagesStorage,
}

impl Module {
    /// Returns a new chat module.
    pub fn new() -> Self {
        Self {
            storage: TextMessagesStorage::new(MESSAGE_MAXIMUM),
        }
    }

    pub fn add_message(&mut self, msg: TextMessage) {
        self.storage.add_message(msg);
    }

    pub fn get_messages(&self) -> Vec<TextMessage>{
        self.storage.get_messages()
    }

    pub fn get_message(&self, id: &U256) -> Option<TextMessage>{
        self.storage.get_message(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Error;

    #[test]
    fn test_new_message() -> Result<(), Error> {
        Ok(())
    }
}
