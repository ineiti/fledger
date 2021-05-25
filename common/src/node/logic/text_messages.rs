/// TextMessages is the structure that holds all known published TextMessages.

use sha2::{Digest, Sha256};

use serde::{Deserialize, Serialize};
use crate::types::U256;

pub struct TextMessages{}

impl TextMessages{
    pub fn new() -> Self{
        Self{}
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextMessage {
    pub src: U256,
    pub created: u64,
    pub liked: u64,
    pub msg: String,
}

impl TextMessage {
    pub fn id(&self) -> U256 {
        let mut id = Sha256::new();
        id.update(&self.src);
        id.update(&self.created.to_le_bytes());
        id.update(&self.liked.to_le_bytes());
        id.update(&self.msg);
        id.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use super::TextMessage;
    use crate::types::U256;

    #[test]
    fn test_id(){
        let tm1 = TextMessage{
            src: U256::rnd(),
            created: 0u64,
            liked: 0u64,
            msg: "test message".to_string(),
        };
        assert_eq!(tm1.id(), tm1.id());

        let mut tm2 = tm1.clone();
        tm2.src = U256::rnd();
        assert_ne!(tm1.id(), tm2.id());

        tm2 = tm1.clone();
        tm2.created = 1u64;
        assert_ne!(tm1.id(), tm2.id());

        tm2 = tm1.clone();
        tm2.liked = 1u64;
        assert_ne!(tm1.id(), tm2.id());

        tm2 = tm1.clone();
        tm2.msg = "short test".to_string();
        assert_ne!(tm1.id(), tm2.id());
    }
}