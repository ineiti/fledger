use crate::crypto::{
    access::{Identity, ACE},
    signer::Verifier,
};

use super::flo::FloWrapper;

pub type FloVerifier = FloWrapper<Box<dyn Verifier>>;

impl FloVerifier {}

pub type FloIdentity = FloWrapper<Identity>;

impl FloIdentity {}

pub type FloACE = FloWrapper<ACE>;

impl FloACE {}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use crate::crypto::{
        access::{ACEData, Condition, Version},
        signer_ed25519::SignerEd25519,
    };

    use super::*;

    #[test]
    fn test_create() {
        let s = SignerEd25519::new();
        let ver = s.verifier();
        let id = Identity::new(
            Condition::Verifier(ver.get_id()),
            Condition::Verifier(ver.get_id()),
        )
        .unwrap();
        let ace = ACE::new(ACEData {
            update: Version::Minimal(id.get_id(), 0),
            rules: HashMap::new(),
            delegation: vec![],
        });
        let va = Version::Minimal(ace.get_id(), 0);
        let _ = FloVerifier::new(va.clone(), ver);
        let _ = FloIdentity::new(va.clone(), id);
        let _ = FloACE::new(va, ace);
    }
}
