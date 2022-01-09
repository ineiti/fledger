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

    /// Returns all ids that are not in our storage
    pub fn filter_known_messages(&self, msgids: Vec<U256>) -> Vec<U256>{
        msgids.iter().filter(|id| self.storage.contains(&id)).cloned().collect()
    }

    /// Adds a message if it's not known yet or not too old.
    /// The return value is if the message has been added.
    pub fn add_message(&mut self, msg: TextMessage) -> bool {
        self.storage.add_message(msg)
    }

    /// Takes a vector of TextMessages and stores the new messages. It returns all
    /// messages that are new.
    pub fn add_messages(&mut self, msgs: Vec<TextMessage>) -> Vec<TextMessage>{
        self.storage.add_messages(msgs)
    }

    /// Gets a copy of all messages stored in the module.
    pub fn get_messages(&self) -> Vec<TextMessage>{
        self.storage.get_messages()
    }

    /// Gets all message-ids that are stored in the module.
    pub fn get_message_ids(&self) -> Vec<U256>{
        self.storage.get_message_ids()
    }

    /// Gets a single message of the module.
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
