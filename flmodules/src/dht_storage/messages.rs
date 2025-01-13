use flarch::{
    broker_io::SubsystemHandler,
    nodeids::{NodeID, U256},
    platform_async_trait,
};
use serde::{Deserialize, Serialize};

use crate::{
    dht_routing::broker::{DHTRoutingIn, DHTRoutingOut},
    flo::{
        dht::{DHTFlo, DHTConfigData},
        flo::FloID,
    },
    overlay::messages::NetworkWrapper,
};

use super::{
    broker::{DHTStorageIn, DHTStorageOut, MODULE_NAME},
    core::*,
};

/// The messages here represent all possible interactions with this module.
#[derive(Debug, Clone)]
pub enum InternIn {
    Routing(DHTRoutingOut),
    Storage(DHTStorageIn),
}

#[derive(Debug, Clone)]
pub enum InternOut {
    Routing(DHTRoutingIn),
    Storage(DHTStorageOut),
}

/// These are the messages which will be exchanged between the nodes for this
/// module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageNodeClosest {
    // Stores the given Flo in the closest node, and all nodes on the route
    // which have enough place left.
    StoreFlo(DHTFlo),
    // Request a flo.
    ReadFlo,
}

/// These are the messages which will be exchanged between the nodes for this
/// module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageNodeDirect {
    // Returns the result of the requested flo.
    ValueFlo(DHTFlo),
    // Indicates this flo is not in the closest node.
    UnknownFlo(FloID),

    // Request a list of all Flos. This is sent to a specific node.
    // TODO: add some filters here like the domain this node accepts
    UpdateInquiry(),
    // Sends out the metadata of available Flos on this node.
    UpdateAvailable(Vec<FloMeta>),
    // Request a list of Flos.
    UpdateRequest(Vec<FloID>),
    // The requested Flos.
    UpdateFlos(Vec<DHTFlo>),
}

/// The message handling part, but only for DHTStorage messages.
#[derive(Debug)]
pub struct DHTStorageMessages {
    pub core: DHTStorageCore,
}

impl DHTStorageMessages {
    /// Returns a new chat module.
    pub fn new(storage: DHTStorageBucket, cfg: DHTConfigData) -> Self {
        Self {
            core: DHTStorageCore::new(storage, cfg),
        }
    }

    fn routing(&mut self, msg: DHTRoutingOut) -> Vec<InternOut> {
        match msg {
            DHTRoutingOut::MessageRouting(origin, _last_hop, _next_hop, key, msg) => msg
                .unwrap_yaml(MODULE_NAME)
                .map(|mn| self.msg_routing(false, origin, key, mn)),
            DHTRoutingOut::MessageClosest(origin, _last_hop, key, msg) => msg
                .unwrap_yaml(MODULE_NAME)
                .map(|mn| self.msg_routing(true, origin, key, mn)),
            DHTRoutingOut::MessageDest(origin, _last_hop, msg) => msg
                .unwrap_yaml(MODULE_NAME)
                .map(|mn| self.msg_dest(origin, mn)),
            _ => None,
        }
        .unwrap_or(vec![])
    }

    fn storage(&mut self, msg: DHTStorageIn) -> Vec<InternOut> {
        let mut out = vec![];
        if let Some(msg) = match msg {
            DHTStorageIn::StoreValue(flo) => {
                self.core.store_kv(flo.clone());
                out.push(DHTStorageOut::UpdateStorage(self.core.storage_clone()).into());
                MessageNodeClosest::StoreFlo(flo.clone()).to_intern_out(flo.id().into())
            }
            DHTStorageIn::ReadValue(id) => match self.core.get_flo(&id) {
                Some(df) => Some(DHTStorageOut::Value(df.clone()).into()),
                None => MessageNodeClosest::ReadFlo.to_intern_out(*id),
            },
        } {
            out.push(msg);
        }
        out
    }

    fn msg_routing(
        &mut self,
        closest: bool,
        origin: NodeID,
        key: U256,
        msg: MessageNodeClosest,
    ) -> Vec<InternOut> {
        match msg {
            MessageNodeClosest::StoreFlo(dhtflo) => {
                self.core.store_kv(dhtflo);
            }
            MessageNodeClosest::ReadFlo => {
                if let Some(df) = self.core.get_flo(&(key.into())) {
                    if closest {
                        return MessageNodeDirect::ValueFlo(df.clone())
                            .to_intern_out(origin)
                            .map_or(vec![], |msg| vec![msg]);
                    }
                }
            }
        }
        vec![]
    }

    fn msg_dest(&mut self, origin: NodeID, msg: MessageNodeDirect) -> Vec<InternOut> {
        match msg {
            MessageNodeDirect::ValueFlo(dhtflo) => Some(DHTStorageOut::Value(dhtflo).into()),
            MessageNodeDirect::UnknownFlo(key) => Some(DHTStorageOut::ValueMissing(key).into()),
            MessageNodeDirect::UpdateInquiry() => {
                let metas = self.core.flo_meta();
                NetworkWrapper::wrap_yaml(MODULE_NAME, &MessageNodeDirect::UpdateAvailable(metas))
                    .ok()
                    .map(|msg| InternOut::Routing(DHTRoutingIn::MessageDirect(origin, msg)))
            }
            MessageNodeDirect::UpdateAvailable(vec) => NetworkWrapper::wrap_yaml(
                MODULE_NAME,
                &MessageNodeDirect::UpdateRequest(self.core.update_available(vec)),
            )
            .ok()
            .map(|msg| InternOut::Routing(DHTRoutingIn::MessageDirect(origin, msg))),
            MessageNodeDirect::UpdateRequest(vec) => NetworkWrapper::wrap_yaml(
                MODULE_NAME,
                &MessageNodeDirect::UpdateFlos(self.core.update_request(vec)),
            )
            .ok()
            .map(|msg| InternOut::Routing(DHTRoutingIn::MessageDirect(origin, msg))),
            MessageNodeDirect::UpdateFlos(vec) => {
                self.core.update_flos(vec);
                None
            }
        }
        .map_or(vec![], |msg| vec![msg])
    }
}

#[platform_async_trait()]
impl SubsystemHandler<InternIn, InternOut> for DHTStorageMessages {
    async fn messages(&mut self, inputs: Vec<InternIn>) -> Vec<InternOut> {
        inputs
            .into_iter()
            .inspect(|msg| log::trace!("In: {msg:?}"))
            .flat_map(|msg| match msg {
                InternIn::Routing(dhtrouting_out) => self.routing(dhtrouting_out),
                InternIn::Storage(dhtstorage_in) => self.storage(dhtstorage_in),
            })
            .inspect(|msg| log::trace!("Out: {msg:?}"))
            .collect()
    }
}

impl MessageNodeClosest {
    fn to_intern_out(&self, dst: NodeID) -> Option<InternOut> {
        serde_yaml::to_string(&self).ok().and_then(|msg| {
            NetworkWrapper::wrap_yaml(MODULE_NAME, &msg)
                .ok()
                .map(|msg_wrap| InternOut::Routing(DHTRoutingIn::MessageClosest(dst, msg_wrap)))
        })
    }
}

impl MessageNodeDirect {
    fn to_intern_out(&self, dst: NodeID) -> Option<InternOut> {
        serde_yaml::to_string(&self).ok().and_then(|msg| {
            NetworkWrapper::wrap_yaml(MODULE_NAME, &msg)
                .ok()
                .map(|msg_wrap| InternOut::Routing(DHTRoutingIn::MessageDirect(dst, msg_wrap)))
        })
    }
}

impl From<DHTStorageOut> for InternOut {
    fn from(value: DHTStorageOut) -> Self {
        InternOut::Storage(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DHTStorageStats {}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    use crate::{crypto::access::Version, flo::testing::{new_ace, new_msg_closest}};

    #[test]
    fn test_choice() -> Result<(), Box<dyn Error>> {
        let [origin, last] = [NodeID::rnd(), NodeID::rnd()];

        let mut msgs = DHTStorageMessages::new(
            DHTStorageBucket::new(origin),
            DHTConfigData {
                over_provide: 1.,
                max_space: 1000,
            },
        );

        let ace = new_ace()?;
        for len in [1, 2, 2] {
            let (_, dout) = new_msg_closest(
                MODULE_NAME,
                origin,
                last,
                Version::Minimal(ace.get_id(), 0),
                "0".repeat(400),
            )?;
            msgs.routing(dout);
            assert_eq!(len, msgs.core.flo_meta().len());
        }

        Ok(())
    }
}
