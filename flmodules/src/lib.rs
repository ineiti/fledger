use bitflags::bitflags;
use serde::{Deserialize, Serialize};
bitflags! {
    #[derive(Clone, Copy, Serialize, Deserialize)]
    pub struct Modules: u32 {
        const ENABLE_STAT = 0b1;
        const ENABLE_RAND = 0b10;
        const ENABLE_GOSSIP = 0b100;
        const ENABLE_PING = 0b1000;
        const ENABLE_WEBPROXY = 0b10000;
    }
}

pub mod nodeconfig;
pub mod template;
pub mod random_connections;
pub mod gossip_events;
pub mod timer;
pub mod ping;
pub mod web_proxy;
