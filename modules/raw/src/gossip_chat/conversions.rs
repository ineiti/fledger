use super::MessageIn;
use crate::random_connections::MessageOut;

impl From<MessageOut> for Option<MessageIn> {
    fn from(mo: MessageOut) -> Option<MessageIn> {
        if let MessageOut::ListUpdate(list) = mo {
            Some(MessageIn::NodeList(list))
        } else {
            None
        }
    }
}
