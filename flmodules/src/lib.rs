use bitflags::bitflags;
use serde::{Deserialize, Serialize};
bitflags! {
    #[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Hash)]
    pub struct Modules: u32 {
        const ENABLE_STAT = 0x1;
        const ENABLE_RAND = 0x2;
        const ENABLE_GOSSIP = 0x4;
        const ENABLE_PING = 0x8;
        const ENABLE_WEBPROXY = 0x10;
        const ENABLE_WEBPROXY_REQUESTS = 0x20;
        const ENABLE_LOOPIX = 0x40;
    }
}

pub mod nodeconfig;
pub mod template;
pub mod random_connections;
pub mod gossip_events;
pub mod timer;
pub mod ping;
pub mod web_proxy;
pub mod network;
pub mod overlay;
pub mod loopix;
