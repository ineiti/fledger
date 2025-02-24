use std::{collections::HashMap, iter};

use bytes::Bytes;
use flarch::nodeids::U256;
use flmacro::AsU256;
use serde::{Deserialize, Serialize};

use crate::{
    signer::{Signature, Signer, SignerError, Verifier},
    tofrombytes::ToFromBytes,
};

use super::signer::KeyPairID;

#[derive(AsU256, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct BadgeID(U256);

/// A Condition is a boolean combination of verifiers and badges
/// which can verify a given signature.
/// A Condition can:
/// - sign a given message
/// - verify a given message
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum Condition {
    Verifier(KeyPairID),
    Badge(Version<BadgeID>),
    NofT(u32, Vec<Condition>),
    /// TODO: re-enable this enum only for "testing"
    /// The Pass condition is always true, but only available during testing.
    Pass,
}

/// A badge can be viewed as an identity or a root of trust.
/// It defines one cryptographic actor which can be composed of
/// many different verifiers and other badges.
pub trait Badge {
    fn badge_id(&self) -> BadgeID;

    fn badge_cond(&self) -> Condition;

    fn badge_version(&self) -> u32;

    fn clone_self(&self) -> Box<dyn Badge>;
}

impl Clone for Box<dyn Badge> {
    fn clone(&self) -> Self {
        self.clone_self()
    }
}

/// What versions of the ACE or badge are accepted.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug, Eq, Hash)]
pub enum Version<T: Serialize + Clone> {
    /// This is the default - everyone should use it and trust future versions.
    Minimal(T, u32),
    /// Paranoid version pinning. Also problematic because old versions might get
    /// forgotten.
    Exact(T, u32),
    /// Just for completeness' sake - this version and all lower version. This doesn't
    /// really make sense.
    Maximal(T, u32),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BadgeSignature {
    cs: CalcSignature,
    msg_orig: Bytes,
    condition: Condition,
}

impl<T: Serialize + Clone> Version<T> {
    pub fn get_id(&self) -> T {
        match self {
            Version::Minimal(id, _) => id.clone(),
            Version::Exact(id, _) => id.clone(),
            Version::Maximal(id, _) => id.clone(),
        }
    }

    pub fn accepts(&self, version: u32) -> bool {
        match self {
            Version::Minimal(_, v) => v <= &version,
            Version::Exact(_, v) => v == &version,
            Version::Maximal(_, v) => v >= &version,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CalcSignature {
    msg: U256,
    condition_hash: Vec<U256>,
    badges: HashMap<Version<BadgeID>, Condition>,
    signatures: HashMap<KeyPairID, Option<(Box<dyn Verifier>, Signature)>>,
}

impl CalcSignature {
    fn new() -> Self {
        CalcSignature {
            msg: U256::zero(),
            condition_hash: vec![],
            badges: HashMap::new(),
            signatures: HashMap::new(),
        }
    }

    pub fn from_cond_badges(
        cond: &Condition,
        badges: HashMap<Version<BadgeID>, Condition>,
        signatures: HashMap<KeyPairID, Option<(Box<dyn Verifier>, Signature)>>,
        msg: Bytes,
    ) -> Result<Self, SignerError> {
        let mut cs = Self::new();
        cs.badges = badges;
        cs.signatures = signatures;
        cs.build_hash_verify(cond, msg)?;
        Ok(cs)
    }

    fn calc_msg(&mut self, msg: Bytes) {
        let parts = iter::once(msg)
            .chain(self.condition_hash.iter().map(|u| u.bytes()))
            .collect::<Vec<Bytes>>();
        self.msg = U256::hash_domain_parts(
            "ConditionHash",
            &parts.iter().map(|b| b.as_ref()).collect::<Vec<_>>(),
        );
    }

    pub fn build_hash_init(
        cond: &Condition,
        msg: Bytes,
        get_badge: &GetBadge,
    ) -> Result<CalcSignature, SignerError> {
        let mut cs = CalcSignature::new();
        cond.build_hash(&mut cs, get_badge, true)?;
        cs.calc_msg(msg);
        Ok(cs)
    }

    fn build_hash_verify(&mut self, cond: &Condition, msg: Bytes) -> Result<(), SignerError> {
        self.condition_hash = vec![];
        cond.build_hash(self, &|_| -> Option<Box<dyn Badge>> { None }, false)?;
        self.calc_msg(msg);
        Ok(())
    }
}

impl<T: Serialize + Clone> Version<T> {
    pub fn id(&self) -> T {
        match self {
            Version::Minimal(id, _) => id.clone(),
            Version::Exact(id, _) => id.clone(),
            Version::Maximal(id, _) => id.clone(),
        }
    }
}

pub type GetBadge = dyn Fn(&Version<BadgeID>) -> Option<Box<dyn Badge>>;

impl Condition {
    fn build_hash(
        &self,
        cs: &mut CalcSignature,
        get_badge: &GetBadge,
        init: bool,
    ) -> Result<(), SignerError> {
        cs.condition_hash.push(self.hash_this());
        Ok(match self {
            Condition::Verifier(verifier) => {
                if init {
                    cs.signatures.insert(verifier.clone(), None);
                }
            }
            Condition::Badge(version) => {
                if init {
                    if cs.badges.contains_key(version) {
                        log::warn!("Condition {self:?} contains loop");
                        return Ok(());
                    }
                    get_badge(version)
                        .ok_or(SignerError::BadgeMissing)?
                        .badge_cond()
                        .build_hash(cs, get_badge, init)?
                } else {
                    let cond = cs
                        .badges
                        .get(version)
                        .ok_or(SignerError::BadgeMissing)?
                        .clone();
                    cond.build_hash(cs, get_badge, init)?
                }
            }
            Condition::NofT(_, vec) => {
                for cond in vec {
                    cond.build_hash(cs, get_badge, init)?;
                }
            }
            Condition::Pass => (),
        })
    }

    fn hash_this(&self) -> U256 {
        U256::hash_data(&self.to_bytes())
    }
}

impl BadgeSignature {
    pub fn from_cond(
        condition: Condition,
        msg_orig: Bytes,
        get_badge: &GetBadge,
    ) -> Result<Self, SignerError> {
        let cs = CalcSignature::build_hash_init(&condition, msg_orig.clone(), get_badge)?;
        Ok(Self {
            cs,
            msg_orig,
            condition,
        })
    }

    pub fn from_calc_signature(cs: CalcSignature, condition: Condition, msg_orig: Bytes) -> Self {
        Self {
            cs,
            msg_orig,
            condition,
        }
    }

    pub fn sign(&mut self, signer: &dyn Signer) -> Result<(), SignerError> {
        let verifier = signer.verifier();
        let signature = signer.sign(&self.cs.msg.bytes())?;
        self.cs
            .signatures
            .entry(verifier.get_id())
            .and_modify(|sig| *sig = Some((verifier, signature)));
        Ok(())
    }

    pub fn is_final(&mut self) -> Result<bool, SignerError> {
        let msg = self.cs.msg;
        self.cs
            .build_hash_verify(&self.condition, self.msg_orig.clone())?;
        if self.cs.msg != msg {
            return Err(SignerError::SigningMessageMismatch);
        }
        self.evaluate_cond(&self.condition)
    }

    fn evaluate_cond(&self, cond: &Condition) -> Result<bool, SignerError> {
        Ok(match cond {
            Condition::Verifier(verifier_id) => self
                .cs
                .signatures
                .get(&verifier_id)
                .and_then(|ver_sig| {
                    ver_sig.as_ref().map(|(ver, sig)| {
                        &ver.get_id() == verifier_id
                            && ver.verify(&self.cs.msg.bytes(), &sig).is_ok()
                    })
                })
                .unwrap_or(false),
            Condition::Badge(version) => self
                .cs
                .badges
                .get(version)
                .and_then(|cond| self.evaluate_cond(cond).ok())
                .unwrap_or(false),
            Condition::NofT(t, vec) => {
                vec.iter()
                    .filter_map(|cond| {
                        self.evaluate_cond(cond)
                            .ok()
                            .and_then(|res| res.then(|| ()))
                    })
                    .count() as u32
                    >= *t
            }
            Condition::Pass => true,
        })
    }

    pub fn finalize(&mut self) -> Result<HashMap<KeyPairID, Signature>, SignerError> {
        if !self.is_final()? {
            return Err(SignerError::SignatureMessageMismatch);
        }
        Ok(self
            .cs
            .signatures
            .clone()
            .into_iter()
            .filter_map(|(k, v)| v.map(|v| (k, v.1)))
            .collect())
    }
}

#[cfg(test)]
mod test {
    use std::error::Error;

    use flarch::start_logging_filter;

    use crate::signer_ed25519::SignerEd25519;

    use super::*;

    #[test]
    fn test_list_paths_errors() -> Result<(), Box<dyn Error>> {
        let msg = Bytes::from("");
        let cond = Condition::Badge(Version::Exact(BadgeID::rnd(), 1));
        assert!(CalcSignature::build_hash_init(&cond, msg.clone(), &|_| None).is_err());
        let cond2 = Condition::NofT(1, vec![cond]);
        assert!(CalcSignature::build_hash_init(&cond2, msg, &|_| None).is_err());
        Ok(())
    }

    struct SVID {
        signer: Box<dyn Signer>,
        _verifier: Box<dyn Verifier>,
        condition: Condition,
        id: KeyPairID,
        // badge: Box<dyn Badge>,
    }

    impl SVID {
        fn new() -> Self {
            let signer = SignerEd25519::new_box();
            let condition = Condition::Verifier(signer.verifier().get_id());
            Self {
                _verifier: signer.verifier(),
                id: signer.verifier().get_id(),
                // badge: Box::new(BT(condition.clone(), 0)),
                condition,
                signer,
            }
        }

        fn condition(&self) -> Condition {
            self.condition.clone()
        }

        // fn signer(&self) -> Box<dyn Signer> {
        //     self.signer.clone()
        // }
    }

    struct BT(Condition, u32);

    impl Badge for BT {
        fn badge_id(&self) -> BadgeID {
            self.0.hash_this().into()
        }

        fn badge_cond(&self) -> Condition {
            self.0.clone()
        }

        fn badge_version(&self) -> u32 {
            self.1
        }

        fn clone_self(&self) -> Box<dyn Badge> {
            Box::new(BT(self.0.clone(), self.1))
        }
    }

    impl BT {
        fn start_signature(
            &self,
            msg: &Bytes,
            get_badge: &GetBadge,
        ) -> Result<BadgeSignature, SignerError> {
            BadgeSignature::from_cond(self.0.clone(), msg.clone(), get_badge)
        }

        fn start_signature_no_badge(&self, msg: &Bytes) -> Result<BadgeSignature, SignerError> {
            self.start_signature(msg, &|_| None)
        }
    }

    #[test]
    fn test_verifiers() -> Result<(), Box<dyn Error>> {
        let msg = Bytes::from("123");
        let msg_bad = Bytes::from("1234");
        let svid0 = SVID::new();
        let svid1 = SVID::new();
        let badge0 = BT(Condition::Verifier(svid0.id), 0);
        let badge1 = BT(Condition::Verifier(svid1.id), 0);
        let mut sig_prep0 = badge0.start_signature_no_badge(&msg)?;
        let mut sig_prep0_bad = badge0.start_signature_no_badge(&msg_bad)?;
        let mut sig_prep1 = badge1.start_signature_no_badge(&msg)?;

        assert_eq!(sig_prep0.cs.msg, sig_prep0.cs.msg);
        assert_ne!(sig_prep0.cs.msg, sig_prep0_bad.cs.msg);
        assert_ne!(sig_prep0.cs.msg, sig_prep1.cs.msg);

        // Wrong and correct signer
        sig_prep0.sign(&*svid1.signer)?;
        assert!(!sig_prep0.is_final()?);
        sig_prep0.sign(&*svid0.signer)?;
        assert!(sig_prep0.is_final()?);

        // Wrong message
        let mut sig_clone = sig_prep0.clone();
        sig_clone.msg_orig = msg_bad.clone();
        assert_eq!(
            Err(SignerError::SigningMessageMismatch),
            sig_clone.is_final()
        );

        // Wrong Condition
        let mut sig_clone = sig_prep0.clone();
        sig_clone.condition = sig_prep1.condition.clone();
        assert_eq!(
            Err(SignerError::SigningMessageMismatch),
            sig_clone.is_final()
        );

        // Signature from different message
        sig_prep0_bad.sign(&*svid0.signer)?;
        assert!(sig_prep0_bad.is_final()?);
        let mut sig_clone = sig_prep0.clone();
        sig_clone.cs.signatures = sig_prep0_bad.cs.signatures.clone();
        assert!(!sig_clone.is_final()?);

        // Signature from different signer
        sig_prep1.sign(&*svid1.signer)?;
        assert!(sig_prep1.is_final()?);
        let mut sig_clone = sig_prep0.clone();
        sig_clone.cs.signatures = sig_prep1.cs.signatures.clone();
        assert!(!sig_clone.is_final()?);

        Ok(())
    }

    #[test]
    fn test_noft() -> Result<(), SignerError> {
        start_logging_filter(vec![]);
        let msg = Bytes::from("123");
        let svid0 = SVID::new();
        let svid1 = SVID::new();
        let svid2 = SVID::new();
        let badge0 = BT(
            Condition::NofT(1, vec![svid0.condition(), svid1.condition()]),
            0,
        );
        let badge1 = BT(
            Condition::NofT(2, vec![svid0.condition(), svid1.condition()]),
            0,
        );
        let badge2 = BT(
            Condition::NofT(
                2,
                vec![svid0.condition(), svid1.condition(), svid2.condition()],
            ),
            0,
        );
        let mut sig_prep0 = badge0.start_signature_no_badge(&msg)?;
        let mut sig_prep1 = badge1.start_signature_no_badge(&msg)?;
        let mut sig_prep2 = badge2.start_signature_no_badge(&msg)?;

        assert_ne!(sig_prep0.cs.msg, sig_prep1.cs.msg);
        assert_ne!(sig_prep0.cs.msg, sig_prep2.cs.msg);
        assert_ne!(sig_prep1.cs.msg, sig_prep2.cs.msg);

        // Wrong signer vs. correct signer
        assert!(!sig_prep0.is_final()?);
        sig_prep0.sign(&*svid2.signer)?;
        assert!(!sig_prep0.is_final()?);
        sig_prep0.sign(&*svid0.signer)?;
        assert!(sig_prep0.is_final()?);

        // Too many signers (is accepted)
        sig_prep0.sign(&*svid1.signer)?;
        assert!(sig_prep0.is_final()?);

        // Second signer
        let mut sig_prep0 = badge0.start_signature_no_badge(&msg)?;
        sig_prep0.sign(&*svid1.signer)?;
        assert!(sig_prep0.is_final()?);

        // Too few signers vs. enough signers
        sig_prep1.sign(&*svid0.signer)?;
        assert!(!sig_prep1.is_final()?);
        sig_prep1.sign(&*svid1.signer)?;
        assert!(sig_prep1.is_final()?);

        // Different signers
        sig_prep2.sign(&*svid0.signer)?;
        sig_prep2.sign(&*svid1.signer)?;
        assert!(sig_prep2.is_final()?);
        for (_, sig) in &mut sig_prep2.cs.signatures {
            *sig = None;
        }
        assert!(!sig_prep2.is_final()?);
        sig_prep2.sign(&*svid0.signer)?;
        sig_prep2.sign(&*svid2.signer)?;
        assert!(sig_prep2.is_final()?);
        for (_, sig) in &mut sig_prep2.cs.signatures {
            *sig = None;
        }
        sig_prep2.sign(&*svid1.signer)?;
        sig_prep2.sign(&*svid2.signer)?;
        assert!(sig_prep2.is_final()?);

        Ok(())
    }

    // #[derive(Default)]
    // struct IDS {
    //     ids: HashMap<Version<BadgeID>, Box<dyn Badge>>,
    // }

    // impl IDS {
    //     fn get_badge(&self, idv: &Version<BadgeID>) -> Option<Box<dyn Badge>> {
    //         self.ids.get(idv).cloned()
    //     }

    //     fn add_badge(&mut self, vers_bid: Version<BadgeID>, vers: u32) -> BT {
    //         let bt = BT(Condition::Badge(vers_bid.clone()), vers);
    //         self.ids.insert(vers_bid, bt.clone_self());
    //         bt
    //     }
    // }

    // #[test]
    // fn test_badge() -> Result<(), Box<dyn Error>> {
    //     let msg = Bytes::from("123");
    //     let svid0 = SVID::new();
    //     let svid1 = SVID::new();
    //     let mut ids = IDS::default();
    //     let badge0_0 = ids.add_badge(Version::Minimal(svid0.badge.badge_id(), 0), 0);
    //     let badge0_1 = ids.add_badge(Version::Minimal(svid0.badge.badge_id(), 1), 1);
    //     let badge1_0 = ids.add_badge(Version::Minimal(svid1.badge.badge_id(), 0), 0);

    //     let mut sig_prep0 = badge0_0.start_signature(&msg, &|vb| ids.get_badge(vb))?;
    //     let mut sig_prep1 = badge0_1.start_signature(&msg, &|vb| ids.get_badge(vb))?;
    //     let mut sig_prep2 = badge1_0.start_signature(&msg, &|vb| ids.get_badge(vb))?;

    //     Ok(())
    // }
}
