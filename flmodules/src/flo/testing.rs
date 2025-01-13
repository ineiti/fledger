use std::{collections::HashMap, error::Error};

use flarch::nodeids::{NodeID, U256};

use crate::{
    crypto::{
        access::{AceId, Condition, Identity, Version, ACEData, ACE},
        signer::SignerError,
        signer_ed25519::SignerEd25519,
    },
    dht_routing::{broker::DHTRoutingOut, kademlia::KNode},
    dht_storage::messages::MessageNodeClosest,
    overlay::messages::NetworkWrapper,
};

use super::{
    dht::DHTFlo,
    flo::{Content, Flo},
};

pub fn new_ace() -> Result<ACE, SignerError> {
    let signer = SignerEd25519::new();
    let cond = Condition::Verifier(signer.verifier().get_id());
    let identity = Identity::new(cond.clone(), cond).unwrap();
    Ok(ACE::new(ACEData {
        update: Version::Minimal(identity.get_id(), 0),
        rules: HashMap::new(),
        delegation: vec![],
    }))
}

pub fn new_dht_flo_blob(va: Version<AceId>, data: String) -> DHTFlo {
    let flo = Flo::new(Content::Blob, data.into(), va);
    DHTFlo{flo, spread: 1}
}

pub fn new_dht_flo_depth(va: Version<AceId>, root: &U256, depth: usize) -> DHTFlo {
    loop {
        let data = format!("{:02x}", U256::rnd());
        let flo = Flo::new(Content::Blob, data.into(), va.clone());
        let nd = KNode::get_depth(root, *flo.id);
        if nd == depth {
            return DHTFlo{flo, spread: 1};
        }
    }
}

pub fn new_wrapper_closest(
    mod_name: &str,
    va: Version<AceId>,
    data: String,
) -> Result<(DHTFlo, NetworkWrapper), Box<dyn Error>> {
    let dht_flo = new_dht_flo_blob(va, data);
    let msg_store = MessageNodeClosest::StoreFlo(dht_flo.clone());
    Ok((
        dht_flo,
        NetworkWrapper::wrap_yaml(mod_name.into(), &msg_store)?,
    ))
}

pub fn new_msg_closest(
    mod_name: &str,
    origin: NodeID,
    last: NodeID,
    va: Version<AceId>,
    data: String,
) -> Result<(DHTFlo, DHTRoutingOut), Box<dyn Error>> {
    let (dht_flo, nm) = new_wrapper_closest(mod_name, va, data)?;
    Ok((
        dht_flo.clone(),
        DHTRoutingOut::MessageClosest(origin, last, *dht_flo.flo.id, nm),
    ))
}
