use std::{collections::HashMap, fmt::Display};

use async_recursion::async_recursion;
use bytes::Bytes;
use flarch::nodeids::U256;
use flmacro::AsU256;
use serde::{Deserialize, Serialize};

use crate::{signer::{Signature, Signer, SignerError, SignerTrait, Verifier, VerifierTrait}, tofrombytes::ToFromBytes};

use super::signer::KeyPairID;

#[derive(AsU256, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct BadgeID(U256);

/// A ConditionLink is a boolean combination of verifiers and badges
/// which can verify a given signature.
/// The ConditionLink only holds the IDs of the Verifiers and the Badges,
/// and is lightweight.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum ConditionLink {
    /// A verifier is the only element of a Condition which can be directly
    /// verified.
    Verifier(KeyPairID),
    Badge(VersionSpec<BadgeID>),
    NofT(u32, Vec<ConditionLink>),
    /// TODO: re-enable this enum only for "testing"
    /// The Pass condition is always true, but only available during testing.
    Fail,
    Pass,
}

/// A Condition represents a full ConditinLink with all IDs replaced by
/// their actual values.
/// TODO: Avoid loops
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum Condition {
    Verifier(Verifier),
    /// The version is needed to verify it's the correct badge.
    Badge(VersionSpec<BadgeID>, Badge),
    NofT(u32, Vec<Condition>),
    NotAvailable,
    Fail,
    Pass,
}

impl Display for Condition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_link().fmt(f)
    }
}

impl Display for ConditionLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self {
            ConditionLink::Verifier(verifier) => format!("Verifier({})", verifier),
            ConditionLink::Badge(version_spec) => format!("Badge({:?})", version_spec),
            ConditionLink::NofT(thr, conditionlinks) => format!("NofT({thr}, [{}])",
        conditionlinks.iter().map(|cond| format!("{cond}")).collect::<Vec<_>>().join(",")),
            ConditionLink::Fail => format!("Fail"),
            ConditionLink::Pass => format!("Pass"),
        })
    }
}

/// A badge can be viewed as an identity or a root of trust.
/// It defines one cryptographic actor which can be composed of
/// many different verifiers and other badges.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct BadgeLink {
    pub id: BadgeID,
    pub version: u32,
    pub condition: ConditionLink,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Badge {
    pub id: BadgeID,
    pub version: u32,
    pub condition: Box<Condition>,
}

/// What versions of the ACE or badge are accepted.
/// TODO: once the U256 becomes a trait that other IDs can inherit,
/// make T depend on U256.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug, Eq, Hash)]
pub enum VersionSpec<T: Serialize + Clone + AsRef<[u8]>> {
    /// This is the default - everyone should use it and trust future versions.
    Minimal(T, u32),
    /// Paranoid version pinning. Also problematic because old versions might get
    /// forgotten.
    Exact(T, u32),
    /// Just for completeness' sake - this version and all lower version. This doesn't
    /// really make sense.
    Maximal(T, u32),
}

unsafe impl<T: Serialize + Clone + AsRef<[u8]>> Send for VersionSpec<T>{}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct ConditionSignature {
    pub condition: Condition,
    pub signatures: HashMap<KeyPairID, Signature>,
}

impl<T: Serialize + Clone + AsRef<[u8]>> VersionSpec<T> {
    pub fn get_id(&self) -> T {
        match self {
            VersionSpec::Minimal(id, _) => id.clone(),
            VersionSpec::Exact(id, _) => id.clone(),
            VersionSpec::Maximal(id, _) => id.clone(),
        }
    }

    pub fn accepts(&self, version: u32) -> bool {
        match self {
            VersionSpec::Minimal(_, v) => v <= &version,
            VersionSpec::Exact(_, v) => v == &version,
            VersionSpec::Maximal(_, v) => v >= &version,
        }
    }
}

impl ConditionLink {
    #[async_recursion(?Send)]
    pub async fn to_condition<B, V>(&self, gb: &B, gv: &V) -> Condition
    where
        B: AsyncFn(VersionSpec<BadgeID>) -> Option<BadgeLink>,
        V: AsyncFn(KeyPairID) -> Option<Verifier>,
    {
        match self {
            ConditionLink::Verifier(ver) => gv(ver.clone())
                .await
                .map(|v| Condition::Verifier(v))
                .unwrap_or(Condition::NotAvailable),
            ConditionLink::Badge(version) => {
                if let Some(b) = gb(version.clone()).await {
                    Condition::Badge(
                        version.clone(),
                        Badge{
                            id: b.id,
                            version: b.version,
                            condition: Box::new(b.condition.to_condition(gb, gv).await),    
                        }
                    )
                } else {
                    Condition::NotAvailable
                }
            }
            ConditionLink::NofT(thr, vec) => {
                let mut conds = vec![];
                for cond in vec {
                    conds.push(cond.to_condition(gb, gv).await)
                }
                Condition::NofT(*thr, conds)
            }
            ConditionLink::Pass => Condition::Pass,
            ConditionLink::Fail => Condition::Fail,
        }
    }
}

impl Condition {
    pub fn hash(&self) -> U256 {
        match self {
            Condition::Verifier(verifier) => U256::hash_domain_parts(
                "flcrypto::Condition::Verifier",
                &[&verifier.get_id().bytes()],
            ),
            Condition::Badge(ver_spec, badge) => U256::hash_domain_parts(
                "flcrypto::Condition::Badge",
                &[&ver_spec.to_rmp_bytes(), &badge.to_rmp_bytes()],
            ),
            Condition::NofT(thr, conditions) => {
                let mut parts: Vec<Bytes> = vec![Bytes::copy_from_slice(&thr.to_le_bytes())];
                for cond in conditions {
                    parts.push(cond.hash().bytes())
                }
                U256::hash_domain_parts(
                    "flcrypto::Condition::NofT",
                    &parts.iter().map(|b| b.as_ref()).collect::<Vec<_>>(),
                )
            }
            Condition::NotAvailable => {
                U256::hash_domain_parts("flcrypto::Condition::NotAvailable", &[])
            }
            Condition::Pass => U256::hash_domain_parts("flcrypto::Condition::Pass", &[]),
            Condition::Fail => U256::hash_domain_parts("flcrypto::Condition::Fail", &[]),
        }
    }

    pub fn equal_link(&self, cond_link: &ConditionLink) -> bool {
        match cond_link {
            ConditionLink::Verifier(key_pair_id) => {
                matches!(self, Condition::Verifier(ver) if &ver.get_id() == key_pair_id)
            }
            ConditionLink::Badge(version_spec) => {
                matches!(self, Condition::Badge(ver_spec, badge) 
                if BadgeLink{ id: badge.id.clone(), version: badge.version, condition: badge.condition.to_link() }
                    .fits_version(version_spec.clone()) && badge.condition.equal_link(cond_link) && version_spec == ver_spec)
            }
            ConditionLink::NofT(_, condition_links) => {
                condition_links.iter().all(|cl| self.equal_link(cl))
            },
            ConditionLink::Pass => matches!(self, Condition::Pass),
            ConditionLink::Fail => matches!(self, Condition::Fail),
        }
    }

    pub fn to_link(&self) -> ConditionLink{
        match self{
            Condition::Verifier(verifier) => ConditionLink::Verifier(verifier.get_id()),
            Condition::Badge(ver_spec, _) => ConditionLink::Badge(ver_spec.clone()),
            Condition::NofT(thr, conditions) => ConditionLink::NofT(*thr, 
            conditions.iter().map(|cond| cond.to_link()).collect::<Vec<_>>()),
            Condition::NotAvailable => todo!(),
            Condition::Pass => ConditionLink::Pass,
            Condition::Fail => ConditionLink::Fail,
        }
    }

    pub fn has_verifier(&self, vid: &KeyPairID) -> bool {
        match self {
            Condition::Verifier(verifier) => &verifier.get_id() == vid,
            Condition::Badge(_, badge) => badge.condition.has_verifier( vid),
            Condition::NofT(_, conditions) => conditions.iter().any(|c| c.has_verifier(vid)),
            Condition::NotAvailable => false,
            Condition::Pass => true,
            Condition::Fail => false,
        }
    }

    pub fn can_sign(&self, vids: &[&KeyPairID]) -> bool {
        if vids.len() > 1 {
            todo!("Check with more than one signer");
        } 
        if let Some(&vid) = vids.first(){
            match self {
                Condition::Verifier(verifier) => &verifier.get_id() == vid,
                Condition::Badge(_, badge) => badge.condition.can_sign( &[vid]),
                Condition::NofT(nbr, conditions) => conditions.iter().any(|c| c.can_sign(&[vid])) && nbr == &1,
                Condition::NotAvailable => false,
                Condition::Pass => true,
                Condition::Fail => false,
            }
        } else {
            self == &Condition::Pass
        }
    }

    pub fn can_signers(&self, vids: &[&Signer]) -> bool {
        let vids = vids.iter().map(|s| s.get_id()).collect::<Vec<_>>();
        self.can_sign(&vids.iter().collect::<Vec<_>>())
    }
}

impl ConditionSignature {
    pub fn new(condition: Condition) -> Self {
        ConditionSignature {
            condition,
            signatures: HashMap::new(),
        }
    }

    pub fn empty() -> Self {
        ConditionSignature{
            condition: Condition::NotAvailable,
            signatures: HashMap::new(),
        }
    }

    pub fn sign_msg(&mut self, sig: &Signer, msg: &Bytes) -> anyhow::Result<()> {
        self.sign_digest(sig, &self.digest(msg))
    }

    pub fn sign_digest(&mut self, sig: &Signer, digest: &U256) -> anyhow::Result<()> {
        if !self.condition.has_verifier( &sig.get_id()) {
            return Err(SignerError::NoSuchSigner.into());
        }
        self.signatures.insert(sig.get_id(), sig.sign(&digest.bytes())?);
        Ok(())
    }

    pub fn signers_msg(&mut self, signers: &[&Signer], msg: &Bytes) -> anyhow::Result<()> {
        let digest = self.digest(msg);
        for sig in signers{
        self.sign_digest(sig, &digest)?
        }
        Ok(())
    }

    pub fn signers_digest(&mut self, signers: &[&Signer], digest: &U256) -> anyhow::Result<()> {
        for sig in signers{
            if !self.condition.has_verifier( &sig.get_id()) {
                return Err(SignerError::NoSuchSigner.into());
            }
            self.signatures.insert(sig.get_id(), sig.sign(&digest.bytes())?);
        }
        Ok(())
    }

    pub fn verify_digest(&self, digest: &U256) -> anyhow::Result<bool> {
        self.verify_cond(&self.condition, &digest)
    }

    pub fn verify_msg(&self, msg: &Bytes) -> anyhow::Result<bool> {
        self.verify_digest(&self.digest(msg))
    }

    fn digest(&self, msg: &Bytes) -> U256 {
        U256::hash_domain_parts(
            "flcrypto::ConditionSignature::sign",
            &[self.condition.hash().as_ref(), msg],
        )
    }

    pub fn verify_cond(&self, cond: &Condition, digest: &U256) -> anyhow::Result<bool> {
        Ok(match cond {
            Condition::Verifier(verifier) => {
                // log::info!("Verifying {:?} with msg {msg:x} and sig {:?}", verifier.get_id(), self.signatures);
                if let Some(sig) = self.signatures.get(&verifier.get_id()) {
                    verifier.verify(&digest.bytes(), sig)?;
                    true
                } else {
                    false
                }
            }
            Condition::Badge(_, badge) => self.verify_cond(&badge.condition, digest)?,
            Condition::NofT(t, vec) => {
                let mut correct = 0;
                for cond in vec {
                    self.verify_cond(cond, digest)?.then(|| correct += 1);
                }
                correct >= *t
            }
            Condition::NotAvailable => false,
            Condition::Pass => true,
            Condition::Fail => false,
        })
    }
}

impl<T: Serialize + Clone + AsRef<[u8]>> VersionSpec<T> {
    pub fn id(&self) -> T {
        match self {
            VersionSpec::Minimal(id, _) => id.clone(),
            VersionSpec::Exact(id, _) => id.clone(),
            VersionSpec::Maximal(id, _) => id.clone(),
        }
    }
}

impl BadgeLink {
    pub fn fits_version(&self, ver: VersionSpec<BadgeID>) -> bool {
        ver.get_id() == self.id && ver.accepts(self.version)
    }
}

#[cfg(test)]
mod test {
    use flarch::{start_logging_filter, start_logging_filter_level};

    use crate::signer_ed25519::SignerEd25519;

    use super::*;

    struct DHT {
        svids: Vec<SVID>,
    }

    impl DHT {
        fn new() -> Self {
            Self { svids: vec![] }
        }

        fn svid(&mut self) -> SVID {
            let s = SVID::new();
            self.svids.push(s.clone());
            s
        }

        async fn get_cond(&self, cl: &ConditionLink) -> Condition {
            cl.to_condition(
                &|id: VersionSpec<BadgeID>| async move {
                    self.svids
                        .iter()
                        .find(|s| s.badge.fits_version(id.clone()))
                        .map(|s| s.badge.clone())
                },
                &|id: KeyPairID| async move {
                    self.svids
                        .iter()
                        .find(|s| s.id == id)
                        .map(|s| s.verifier.clone())
                },
            )
            .await
        }

        async fn get_cond_sig(&self, cl: &ConditionLink) -> ConditionSignature {
            ConditionSignature::new(self.get_cond(cl).await)
        }
    }

    #[derive(Clone)]
    struct SVID {
        signer: Signer,
        verifier: Verifier,
        condition: ConditionLink,
        id: KeyPairID,
        badge: BadgeLink,
    }

    impl SVID {
        fn new() -> Self {
            let signer = SignerEd25519::new();
            let condition = ConditionLink::Verifier(signer.verifier().get_id());
            Self {
                badge: BadgeLink {
                    id: BadgeID::rnd(),
                    version: 0,
                    condition: ConditionLink::Verifier(signer.get_id()),
                },
                verifier: signer.verifier(),
                id: signer.verifier().get_id(),
                condition,
                signer,
            }
        }

        fn condition(&self) -> ConditionLink {
            self.condition.clone()
        }

        // fn signer(&self) -> Signer {
        //     self.signer.clone()
        // }
    }

    #[tokio::test]
    async fn test_sign() -> anyhow::Result<()> {
        start_logging_filter_level(vec![], log::LevelFilter::Trace);
        let msg = Bytes::from("123");
        let msg_bad = Bytes::from("1234");
        let mut dht = DHT::new();
        let svid0 = dht.svid();
        let svid1 = dht.svid();

        let mut cond_sig0 = dht.get_cond_sig(&svid0.condition).await;
        let cond_sig1 = dht.get_cond_sig(&svid1.condition).await;

        // Wrong and correct signer
        assert_eq!(
            anyhow::anyhow!(SignerError::NoSuchSigner).to_string(),
            cond_sig0.sign_msg(&svid1.signer, &msg).unwrap_err().to_string()
        );
        assert!(!cond_sig0.verify_msg(&msg)?);
        cond_sig0.sign_msg(&svid0.signer, &msg)?;
        assert!(cond_sig0.verify_msg(&msg)?);

        // Condition swap
        let mut cond_clone = cond_sig0.clone();
        log::info!("{:?} - {:?}", cond_sig0.condition.hash(), cond_sig1.condition.hash());
        cond_clone.condition = cond_sig1.condition;
        assert!(!cond_clone.verify_msg(&msg)?);

        // Wrong message
        assert_eq!(
            anyhow::anyhow!(SignerError::SignatureMessageMismatch).to_string(),
            cond_sig0.verify_msg(&msg_bad).unwrap_err().to_string()
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_noft() -> anyhow::Result<()> {
        start_logging_filter(vec![]);
        let msg = Bytes::from("123");
        let mut dht = DHT::new();
        let svid0 = dht.svid();
        let svid1 = dht.svid();
        let svid2 = dht.svid();
        let cond_link0 = ConditionLink::NofT(1, vec![svid0.condition(), svid1.condition()]);
        let cond_link1 = ConditionLink::NofT(2, vec![svid0.condition(), svid1.condition()]);
        let cond_link2 = ConditionLink::NofT(
            2,
            vec![svid0.condition(), svid1.condition(), svid2.condition()],
        );
        let mut cond_sig0 = dht.get_cond_sig(&cond_link0).await;
        let mut cond_sig1 = dht.get_cond_sig(&cond_link1).await;
        let mut cond_sig2 = dht.get_cond_sig(&cond_link2).await;

        assert_ne!(cond_sig0.digest(&msg), cond_sig1.digest(&msg));
        assert_ne!(cond_sig0.digest(&msg), cond_sig2.digest(&msg));
        assert_ne!(cond_sig1.digest(&msg), cond_sig2.digest(&msg));

        // Wrong signer vs. correct signer
        assert!(!cond_sig0.verify_msg(&msg)?);
        assert_eq!(
            anyhow::anyhow!(SignerError::NoSuchSigner).to_string(),
            cond_sig0.sign_msg(&svid2.signer, &msg).unwrap_err().to_string()
        );
        cond_sig0.sign_msg(&svid0.signer, &msg)?;
        assert!(cond_sig0.verify_msg(&msg)?);

        // Too many signers (is accepted)
        cond_sig0.sign_msg(&svid1.signer, &msg)?;
        assert!(cond_sig0.verify_msg(&msg)?);

        // Second signer
        cond_sig0.signatures = HashMap::new();
        cond_sig0.sign_msg(&svid1.signer, &msg)?;
        assert!(cond_sig0.verify_msg(&msg)?);

        // Too few signers vs. enough signers
        cond_sig1.sign_msg(&svid0.signer, &msg)?;
        assert!(!cond_sig1.verify_msg(&msg)?);
        cond_sig1.sign_msg(&svid1.signer, &msg)?;
        assert!(cond_sig1.verify_msg(&msg)?);

        // Different signers
        cond_sig2.sign_msg(&svid0.signer, &msg)?;
        cond_sig2.sign_msg(&svid1.signer, &msg)?;
        assert!(cond_sig2.verify_msg(&msg)?);
        cond_sig2.signatures = HashMap::new();
        assert!(!cond_sig2.verify_msg(&msg)?);
        cond_sig2.sign_msg(&svid0.signer, &msg)?;
        cond_sig2.sign_msg(&svid2.signer, &msg)?;
        assert!(cond_sig2.verify_msg(&msg)?);
        cond_sig2.signatures = HashMap::new();
        cond_sig2.sign_msg(&svid1.signer, &msg)?;
        cond_sig2.sign_msg(&svid2.signer, &msg)?;
        assert!(cond_sig2.verify_msg(&msg)?);

        Ok(())
    }

    #[test]
    fn serialize_condsig() -> anyhow::Result<()>{
        let signer = SignerEd25519::new();
        let mut sig = ConditionSignature{
            condition: Condition::Verifier(signer.verifier()),
            signatures: HashMap::new(),
        };
        sig.sign_digest(&signer, &U256::rnd())?;

        let sig_str = serde_yaml::to_string(&sig)?;
        let sig2: ConditionSignature = serde_yaml::from_str(&sig_str)?;

        assert_eq!(sig, sig2);
        Ok(())
    }
}
