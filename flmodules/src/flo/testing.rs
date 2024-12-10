use std::error::Error;

use flarch::nodeids::{NodeID, U256};

use crate::{
    crypto::{
        entity::{Entity, EntityError},
        signer::{SignatureType, Signer},
    },
    dht_routing::{broker::DHTRoutingOut, kademlia::KNode},
    dht_storage::messages::MessageNodeClosest,
    overlay::messages::NetworkWrapper,
};

use super::{
    dht::DHTFlo,
    flo::{Action, Condition, Content, Flo, Rule, ACE},
};

pub fn new_ace() -> Result<ACE, EntityError> {
    let signer = Signer::new(SignatureType::Ed25519);
    let entity = Entity::new(&[signer.verifier().into()], 1)?;
    Ok(ACE {
        rules: vec![Rule {
            action: Action::UpdateACE,
            condition: Condition::Signature(entity.get_id()),
        }],
    })
}

pub fn new_dht_flo_blob(ace: ACE, data: String) -> DHTFlo {
    let flo = Flo::new_now(Content::Blob, data, ace);
    DHTFlo::new(flo, 1)
}

pub fn new_dht_flo_depth(ace: ACE, root: &U256, depth: usize) -> DHTFlo {
    loop {
        let data = format!("{:02x}", U256::rnd());
        let flo = Flo::new_now(Content::Blob, data, ace.clone());
        let nd = KNode::get_depth(root, flo.id);
        if nd == depth {
            return DHTFlo::new(flo, 1);
        }
    }
}

pub fn new_wrapper_closest(
    mod_name: &str,
    ace: ACE,
    data: String,
) -> Result<(DHTFlo, NetworkWrapper), Box<dyn Error>> {
    let dht_flo = new_dht_flo_blob(ace, data);
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
    ace: &ACE,
    data: String,
) -> Result<(DHTFlo, DHTRoutingOut), Box<dyn Error>> {
    let (dht_flo, nm) = new_wrapper_closest(mod_name, ace.clone(), data)?;
    Ok((
        dht_flo.clone(),
        DHTRoutingOut::MessageClosest(origin, last, dht_flo.id(), nm),
    ))
}
