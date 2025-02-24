use bitflags::bitflags;
use serde::{Deserialize, Serialize};
bitflags! {
    #[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Hash)]
    pub struct Modules: u32 {
        const STAT = 0x1;
        const RAND = 0x2;
        const GOSSIP = 0x4;
        const PING = 0x8;
        const WEBPROXY = 0x10;
        const WEBPROXY_REQUESTS = 0x20;
        const DHT_ROUTER = 0x40;
        const DHT_STORAGE = 0x80;
    }
}

pub mod dht_router;
pub mod dht_storage;
pub mod flo;
pub mod gossip_events;
pub mod network;
pub mod nodeconfig;
pub mod router;
pub mod ping;
pub mod random_connections;
pub mod template;
pub mod timer;
pub mod web_proxy;

pub mod testing;
